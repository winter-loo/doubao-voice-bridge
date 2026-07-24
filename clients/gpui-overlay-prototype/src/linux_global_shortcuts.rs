use std::thread;

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
const VOICE_SHORTCUT_TRIGGER: &str = "F13";

#[derive(Debug)]
pub enum PortalShortcutError {
    Cancelled,
    Unavailable(String),
    Failed(String),
}

impl std::fmt::Display for PortalShortcutError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Cancelled => formatter.write_str("global shortcut authorization was cancelled"),
            Self::Unavailable(error) => {
                write!(formatter, "global shortcuts portal unavailable: {error}")
            }
            Self::Failed(error) => write!(formatter, "global shortcuts portal failed: {error}"),
        }
    }
}

pub fn start(
    on_activated: impl Fn() + Send + 'static,
    on_error: impl FnOnce(PortalShortcutError) + Send + 'static,
) {
    thread::Builder::new()
        .name("doubao-global-shortcuts-portal".to_string())
        .spawn(move || {
            if let Err(error) = future::block_on(run(on_activated)) {
                on_error(error);
            }
        })
        .expect("failed to start global shortcuts portal thread");
}

async fn run(on_activated: impl Fn()) -> Result<(), PortalShortcutError> {
    register_application_id().await;

    let portal = GlobalShortcuts::new().await.map_err(classify_error)?;
    let mut activated = portal.receive_activated().await.map_err(classify_error)?;
    let session = portal.create_session().await.map_err(classify_error)?;
    let shortcut = NewShortcut::new(VOICE_SHORTCUT_ID, VOICE_SHORTCUT_DESCRIPTION)
        .preferred_trigger(VOICE_SHORTCUT_TRIGGER);
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
    if let Err(error) = register_host_app(app_id).await {
        eprintln!(
            "[linux-client] could not register host application ID; \
             continuing with portal auto-detection: {error}"
        );
    }
}

fn classify_error(error: Error) -> PortalShortcutError {
    match error {
        Error::Response(ResponseError::Cancelled) | Error::Portal(PortalError::Cancelled(_)) => {
            PortalShortcutError::Cancelled
        }
        Error::PortalNotFound(_) => PortalShortcutError::Unavailable(error.to_string()),
        _ => PortalShortcutError::Failed(error.to_string()),
    }
}
