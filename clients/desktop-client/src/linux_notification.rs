use std::collections::HashMap;

use zbus::zvariant::Value;

use crate::CLIENT_APP_ID;

/// The desktop notification service every freedesktop session provides.
#[zbus::proxy(
    interface = "org.freedesktop.Notifications",
    default_service = "org.freedesktop.Notifications",
    default_path = "/org/freedesktop/Notifications"
)]
trait Notifications {
    #[allow(clippy::too_many_arguments)]
    fn notify(
        &self,
        app_name: &str,
        replaces_id: u32,
        app_icon: &str,
        summary: &str,
        body: &str,
        actions: &[&str],
        hints: HashMap<&str, Value<'_>>,
        expire_timeout: i32,
    ) -> zbus::Result<u32>;
}

const EXPIRE_AFTER_MILLISECONDS: i32 = 8_000;

/// Tells the user something the client cannot show any other way. The overlay
/// is a capsule with no room for words, and the session log is not in front of
/// anyone, so a silent failure would otherwise look like lost text.
pub fn show(summary: &str, body: &str) {
    if let Err(error) = deliver(summary, body) {
        eprintln!("[linux-client] could not show a desktop notification: {error}");
    }
}

fn deliver(summary: &str, body: &str) -> Result<(), String> {
    let connection = zbus::blocking::Connection::session()
        .map_err(|error| format!("could not reach the session bus: {error}"))?;
    let notifications = NotificationsProxyBlocking::new(&connection)
        .map_err(|error| format!("could not address the notification service: {error}"))?;
    notifications
        .notify(
            "豆包语音输入",
            0,
            CLIENT_APP_ID,
            summary,
            body,
            &[],
            HashMap::new(),
            EXPIRE_AFTER_MILLISECONDS,
        )
        .map(|_| ())
        .map_err(|error| error.to_string())
}
