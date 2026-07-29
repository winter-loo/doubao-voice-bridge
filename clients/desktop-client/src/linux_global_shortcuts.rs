use std::{
    env, fs, io,
    path::{Path, PathBuf},
    thread,
    time::Duration,
};

use ashpd::{
    AppID, Error, PortalError,
    desktop::{
        ResponseError,
        global_shortcuts::{GlobalShortcuts, NewShortcut},
    },
    register_host_app,
};
use futures_lite::{StreamExt as _, future};

use crate::CLIENT_APP_ID;

const VOICE_SHORTCUT_ID: &str = "toggle-voice-input";
const VOICE_SHORTCUT_DESCRIPTION: &str = "切换豆包语音输入";
const DESKTOP_FILE_NAME: &str = "local.doubao.voicebridge.desktop";
const PORTAL_RETRY_DELAY: Duration = Duration::from_millis(750);

#[derive(Debug)]
pub enum PortalShortcutError {
    Cancelled,
    Unavailable(String),
    Transient(String),
    Failed(String),
}

impl std::fmt::Display for PortalShortcutError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Cancelled => formatter.write_str("global shortcut authorization was cancelled"),
            Self::Unavailable(error) => {
                write!(formatter, "global shortcuts portal unavailable: {error}")
            }
            Self::Transient(error) | Self::Failed(error) => {
                write!(formatter, "global shortcuts portal failed: {error}")
            }
        }
    }
}

pub fn start(
    preferred_trigger: String,
    on_activated: impl Fn() + Send + 'static,
    on_error: impl FnOnce(PortalShortcutError) + Send + 'static,
) {
    thread::Builder::new()
        .name("doubao-global-shortcuts-portal".to_string())
        .spawn(move || {
            future::block_on(register_application_id());
            let mut retries = 0;
            loop {
                let error = match future::block_on(run(&preferred_trigger, &on_activated)) {
                    Ok(()) => break,
                    Err(error) => error,
                };
                let Some(delay) = retry_delay(&error, retries) else {
                    on_error(error);
                    break;
                };
                retries += 1;
                eprintln!(
                    "[linux-client] {error}; retrying portal registration in {}ms",
                    delay.as_millis()
                );
                thread::sleep(delay);
            }
        })
        .expect("failed to start global shortcuts portal thread");
}

async fn run(preferred_trigger: &str, on_activated: &impl Fn()) -> Result<(), PortalShortcutError> {
    let portal = GlobalShortcuts::new().await.map_err(classify_error)?;
    let mut activated = portal.receive_activated().await.map_err(classify_error)?;
    let session = portal.create_session().await.map_err(classify_error)?;
    let shortcut = NewShortcut::new(VOICE_SHORTCUT_ID, VOICE_SHORTCUT_DESCRIPTION)
        .preferred_trigger(preferred_trigger);
    let request = portal
        .bind_shortcuts(&session, &[shortcut], None)
        .await
        .map_err(classify_error)?;
    let response = request.response().map_err(classify_error)?;
    let bound = response
        .shortcuts()
        .iter()
        .find(|shortcut| shortcut.id() == VOICE_SHORTCUT_ID)
        .ok_or_else(|| {
            PortalShortcutError::Failed(
                "the desktop did not bind the voice input shortcut".to_string(),
            )
        })?;

    eprintln!(
        "[linux-client] global shortcut ready: {}",
        bound.trigger_description()
    );

    while let Some(event) = activated.next().await {
        if event.shortcut_id() == VOICE_SHORTCUT_ID {
            on_activated();
        }
    }

    Err(PortalShortcutError::Failed(
        "the portal activation stream ended".to_string(),
    ))
}

async fn register_application_id() {
    let Ok(app_id) = AppID::try_from(CLIENT_APP_ID) else {
        eprintln!("[linux-client] invalid portal application ID: {CLIENT_APP_ID}");
        return;
    };
    match ensure_desktop_entry() {
        Ok(Some(path)) => eprintln!(
            "[linux-client] installed portal desktop entry at {}",
            path.display()
        ),
        Ok(None) => {}
        Err(error) => eprintln!(
            "[linux-client] could not install portal desktop entry; \
             continuing with portal auto-detection: {error}"
        ),
    }
    if let Err(error) = register_host_app(app_id).await {
        eprintln!(
            "[linux-client] could not register host application ID; \
             continuing with portal auto-detection: {error}"
        );
    }
}

fn ensure_desktop_entry() -> io::Result<Option<PathBuf>> {
    let applications_dir = user_data_home()?.join("applications");
    let desktop_path = applications_dir.join(DESKTOP_FILE_NAME);

    if desktop_path.exists() {
        return Ok(None);
    }

    fs::create_dir_all(&applications_dir)?;
    fs::write(&desktop_path, desktop_entry_contents()?)?;
    Ok(Some(desktop_path))
}

fn user_data_home() -> io::Result<PathBuf> {
    if let Some(data_home) = env::var_os("XDG_DATA_HOME").filter(|value| !value.is_empty()) {
        return Ok(PathBuf::from(data_home));
    }

    let Some(home) = env::var_os("HOME").filter(|value| !value.is_empty()) else {
        return Err(io::Error::new(
            io::ErrorKind::NotFound,
            "HOME is not set, and XDG_DATA_HOME is unavailable",
        ));
    };
    Ok(PathBuf::from(home).join(".local/share"))
}

fn desktop_entry_contents() -> io::Result<String> {
    let executable = env::current_exe()?;
    let icon = icon_path();
    let icon_line = icon
        .as_ref()
        .map(|path| format!("Icon={}\n", path.to_string_lossy()))
        .unwrap_or_default();

    Ok(format!(
        "[Desktop Entry]\n\
         Type=Application\n\
         Version=1.0\n\
         Name=Doubao Voice Client\n\
         Comment=Doubao voice input overlay\n\
         Exec={}\n\
         {icon_line}\
         Terminal=false\n\
         NoDisplay=true\n\
         Categories=Utility;\n\
         StartupNotify=false\n",
        desktop_exec_arg(&executable)
    ))
}

fn icon_path() -> Option<PathBuf> {
    let path = env::current_dir()
        .ok()?
        .join("assets")
        .join("doubao-voice-client.png");
    path.exists().then_some(path)
}

fn desktop_exec_arg(path: &Path) -> String {
    let mut escaped = String::from("\"");
    for character in path.to_string_lossy().chars() {
        match character {
            '"' | '\\' | '$' | '`' => {
                escaped.push('\\');
                escaped.push(character);
            }
            '%' => escaped.push_str("%%"),
            _ => escaped.push(character),
        }
    }
    escaped.push('"');
    escaped
}

fn classify_error(error: Error) -> PortalShortcutError {
    match error {
        Error::Response(ResponseError::Cancelled) | Error::Portal(PortalError::Cancelled(_)) => {
            PortalShortcutError::Cancelled
        }
        Error::Response(ResponseError::Other) => PortalShortcutError::Transient(error.to_string()),
        Error::PortalNotFound(_) => PortalShortcutError::Unavailable(error.to_string()),
        _ => PortalShortcutError::Failed(error.to_string()),
    }
}

fn retry_delay(error: &PortalShortcutError, retries: usize) -> Option<Duration> {
    (retries == 0 && matches!(error, PortalShortcutError::Transient(_)))
        .then_some(PORTAL_RETRY_DELAY)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn portal_other_is_retried_once() {
        let error = classify_error(Error::Response(ResponseError::Other));

        assert_eq!(retry_delay(&error, 0), Some(PORTAL_RETRY_DELAY));
        assert_eq!(retry_delay(&error, 1), None);
    }
}
