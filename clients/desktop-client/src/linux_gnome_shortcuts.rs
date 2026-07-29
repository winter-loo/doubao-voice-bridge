use std::{env, ffi::OsStr, fs, io, os::unix::net::UnixDatagram, path::PathBuf, sync::Arc, thread};

use gio::{
    Settings,
    prelude::{SettingsExt as _, SettingsExtManual as _},
};

use crate::linux_shortcut::LinuxShortcut;

const TOGGLE_ARGUMENT: &str = "--toggle-running";
const TOGGLE_MESSAGE: &[u8] = b"toggle";
const SOCKET_FILE_NAME: &str = "doubao-voice-client.sock";
const MEDIA_KEYS_SCHEMA: &str = "org.gnome.settings-daemon.plugins.media-keys";
const CUSTOM_KEYBINDING_SCHEMA: &str =
    "org.gnome.settings-daemon.plugins.media-keys.custom-keybinding";
const CUSTOM_KEYBINDING_PATH: &str =
    "/org/gnome/settings-daemon/plugins/media-keys/custom-keybindings/doubao-voice-client/";

pub fn forward_toggle_invocation() -> bool {
    if !is_toggle_invocation(env::args_os()) {
        return false;
    }
    if let Err(error) = notify_running_instance() {
        eprintln!("[linux-client] could not activate the running client: {error}");
    }
    true
}

pub fn is_gnome_desktop() -> bool {
    env::var("XDG_CURRENT_DESKTOP").is_ok_and(|desktop| {
        desktop
            .split(':')
            .any(|name| name.eq_ignore_ascii_case("gnome"))
    })
}

pub fn start(
    shortcut: &LinuxShortcut,
    on_activated: impl Fn() + Send + Sync + 'static,
) -> Result<(), String> {
    let socket_path = runtime_socket_path()?;
    remove_stale_socket(&socket_path)?;
    let socket = UnixDatagram::bind(&socket_path).map_err(|error| {
        format!(
            "could not bind shortcut socket {}: {error}",
            socket_path.display()
        )
    })?;

    install_custom_keybinding(shortcut)?;

    let on_activated = Arc::new(on_activated);
    thread::Builder::new()
        .name("doubao-gnome-shortcut".to_string())
        .spawn(move || receive_activations(socket, on_activated))
        .map_err(|error| format!("could not start GNOME shortcut listener: {error}"))?;

    Ok(())
}

fn is_toggle_invocation<I, S>(args: I) -> bool
where
    I: IntoIterator<Item = S>,
    S: AsRef<OsStr>,
{
    let mut args = args.into_iter();
    let _executable = args.next();
    args.next()
        .is_some_and(|argument| argument.as_ref() == OsStr::new(TOGGLE_ARGUMENT))
        && args.next().is_none()
}

fn notify_running_instance() -> Result<(), String> {
    let socket = UnixDatagram::unbound()
        .map_err(|error| format!("could not create shortcut socket: {error}"))?;
    let path = runtime_socket_path()?;
    socket
        .send_to(TOGGLE_MESSAGE, &path)
        .map_err(|error| format!("could not send activation to {}: {error}", path.display()))?;
    Ok(())
}

fn runtime_socket_path() -> Result<PathBuf, String> {
    env::var_os("XDG_RUNTIME_DIR")
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
        .map(|directory| directory.join(SOCKET_FILE_NAME))
        .ok_or_else(|| "XDG_RUNTIME_DIR is not set".to_string())
}

fn remove_stale_socket(path: &PathBuf) -> Result<(), String> {
    match fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(format!(
            "could not remove stale shortcut socket {}: {error}",
            path.display()
        )),
    }
}

fn receive_activations(socket: UnixDatagram, on_activated: Arc<dyn Fn() + Send + Sync + 'static>) {
    let mut message = [0_u8; 32];
    loop {
        match socket.recv(&mut message) {
            Ok(length) if &message[..length] == TOGGLE_MESSAGE => on_activated(),
            Ok(_) => {}
            Err(error) => {
                eprintln!("[linux-client] GNOME shortcut listener stopped: {error}");
                return;
            }
        }
    }
}

fn install_custom_keybinding(shortcut: &LinuxShortcut) -> Result<(), String> {
    let schema_source = gio::SettingsSchemaSource::default()
        .ok_or_else(|| "GNOME settings schema source is unavailable".to_string())?;
    for schema in [MEDIA_KEYS_SCHEMA, CUSTOM_KEYBINDING_SCHEMA] {
        if schema_source.lookup(schema, true).is_none() {
            return Err(format!("GNOME settings schema {schema} is unavailable"));
        }
    }

    let executable = env::current_exe()
        .map_err(|error| format!("could not locate the client executable: {error}"))?;
    let mut command = glib::shell_quote(executable);
    command.push(" ");
    command.push(TOGGLE_ARGUMENT);
    let command = command
        .into_string()
        .map_err(|_| "the client executable path is not valid UTF-8".to_string())?;

    let binding = Settings::with_path(CUSTOM_KEYBINDING_SCHEMA, CUSTOM_KEYBINDING_PATH);
    binding
        .set_string("name", "Doubao Voice Input")
        .map_err(|error| format!("could not set GNOME shortcut name: {error}"))?;
    binding
        .set_string("command", &command)
        .map_err(|error| format!("could not set GNOME shortcut command: {error}"))?;
    binding
        .set_string("binding", &shortcut.gtk_accelerator())
        .map_err(|error| format!("could not set GNOME shortcut binding: {error}"))?;

    let media_keys = Settings::new(MEDIA_KEYS_SCHEMA);
    let mut paths = media_keys
        .strv("custom-keybindings")
        .iter()
        .map(ToString::to_string)
        .collect::<Vec<_>>();
    if !paths.iter().any(|path| path == CUSTOM_KEYBINDING_PATH) {
        paths.push(CUSTOM_KEYBINDING_PATH.to_string());
        media_keys
            .set_strv("custom-keybindings", paths)
            .map_err(|error| format!("could not register GNOME shortcut path: {error}"))?;
    }
    Settings::sync();
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::ffi::OsString;

    use super::*;

    #[test]
    fn recognizes_only_the_internal_toggle_invocation() {
        let toggle = [
            OsString::from("DoubaoVoiceClient"),
            OsString::from("--toggle-running"),
        ];
        let normal = [OsString::from("DoubaoVoiceClient")];

        assert!(is_toggle_invocation(toggle));
        assert!(!is_toggle_invocation(normal));
    }
}
