use std::{
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
        mpsc::{self, RecvTimeoutError},
    },
    thread,
    time::Duration,
};

use gtk::glib;
use tray_icon::{
    Icon, TrayIcon, TrayIconBuilder,
    menu::{Menu, MenuEvent, MenuItem, PredefinedMenuItem},
};

const TOGGLE_MENU_ID: &str = "toggle-voice-input";
const QUIT_MENU_ID: &str = "quit";
const TRAY_ICON_SIZE: u32 = 32;
const TRAY_START_TIMEOUT: Duration = Duration::from_secs(5);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TrayPhase {
    Idle,
    Activating,
    Listening,
    Finishing,
}

pub fn start(
    phase: fn() -> TrayPhase,
    on_toggle: impl Fn() + Send + Sync + 'static,
    on_quit: impl Fn() + Send + Sync + 'static,
) -> Result<(), String> {
    let (ready_tx, ready_rx) = mpsc::sync_channel(1);
    let cancelled = Arc::new(AtomicBool::new(false));
    let thread_cancelled = Arc::clone(&cancelled);
    thread::Builder::new()
        .name("doubao-linux-tray".to_string())
        .spawn(move || match initialize(phase, on_toggle, on_quit) {
            Ok(tray) => {
                if thread_cancelled.load(Ordering::Acquire) || ready_tx.send(Ok(())).is_err() {
                    return;
                }
                gtk::main();
                drop(tray);
            }
            Err(error) => {
                let _ = ready_tx.send(Err(error));
            }
        })
        .map_err(|error| format!("could not start tray thread: {error}"))?;

    match ready_rx.recv_timeout(TRAY_START_TIMEOUT) {
        Ok(result) => result,
        Err(RecvTimeoutError::Timeout) => {
            cancelled.store(true, Ordering::Release);
            Err(format!(
                "tray initialization timed out after {} seconds",
                TRAY_START_TIMEOUT.as_secs()
            ))
        }
        Err(RecvTimeoutError::Disconnected) => {
            Err("tray thread stopped during initialization".to_string())
        }
    }
}

fn initialize(
    phase: fn() -> TrayPhase,
    on_toggle: impl Fn() + Send + Sync + 'static,
    on_quit: impl Fn() + Send + Sync + 'static,
) -> Result<TrayIcon, String> {
    gtk::init().map_err(|error| format!("could not initialize GTK: {error}"))?;

    let menu = Menu::new();
    let labels = tray_labels(phase());
    let status_item = MenuItem::new(labels.status, false, None);
    let toggle_item = MenuItem::with_id(TOGGLE_MENU_ID, labels.toggle, labels.can_toggle, None);
    let separator = PredefinedMenuItem::separator();
    let quit_item = MenuItem::with_id(QUIT_MENU_ID, "退出", true, None);
    menu.append_items(&[&status_item, &toggle_item, &separator, &quit_item])
        .map_err(|error| format!("could not build tray menu: {error}"))?;

    let tray = TrayIconBuilder::new()
        .with_id("doubao-voice-client")
        .with_menu(Box::new(menu))
        .with_icon(tray_icon()?)
        .with_tooltip("Doubao Voice Client")
        .build()
        .map_err(|error| format!("could not create tray icon: {error}"))?;

    MenuEvent::set_event_handler(Some(move |event: MenuEvent| match event.id().as_ref() {
        TOGGLE_MENU_ID => on_toggle(),
        QUIT_MENU_ID => on_quit(),
        _ => {}
    }));

    glib::timeout_add_local(Duration::from_millis(150), move || {
        let labels = tray_labels(phase());
        status_item.set_text(labels.status);
        toggle_item.set_text(labels.toggle);
        toggle_item.set_enabled(labels.can_toggle);
        glib::ControlFlow::Continue
    });

    Ok(tray)
}

struct TrayLabels {
    status: &'static str,
    toggle: &'static str,
    can_toggle: bool,
}

fn tray_labels(phase: TrayPhase) -> TrayLabels {
    match phase {
        TrayPhase::Idle => TrayLabels {
            status: "状态：待机",
            toggle: "开始语音输入",
            can_toggle: true,
        },
        TrayPhase::Activating => TrayLabels {
            status: "状态：正在激活",
            toggle: "取消语音输入",
            can_toggle: true,
        },
        TrayPhase::Listening => TrayLabels {
            status: "状态：正在录音",
            toggle: "结束语音输入",
            can_toggle: true,
        },
        TrayPhase::Finishing => TrayLabels {
            status: "状态：正在处理识别结果",
            toggle: "正在处理…",
            can_toggle: false,
        },
    }
}

fn tray_icon() -> Result<Icon, String> {
    Icon::from_rgba(
        tray_icon_rgba(TRAY_ICON_SIZE),
        TRAY_ICON_SIZE,
        TRAY_ICON_SIZE,
    )
    .map_err(|error| format!("could not create tray icon pixels: {error}"))
}

fn tray_icon_rgba(size: u32) -> Vec<u8> {
    let mut rgba = vec![0; (size * size * 4) as usize];
    let center = (size as f32 - 1.0) / 2.0;
    let radius = size as f32 * 0.47;

    for y in 0..size {
        for x in 0..size {
            let offset = ((y * size + x) * 4) as usize;
            let dx = x as f32 - center;
            let dy = y as f32 - center;
            if dx * dx + dy * dy <= radius * radius {
                rgba[offset..offset + 4].copy_from_slice(&[44, 153, 255, 255]);
            }

            let microphone = (x >= size * 11 / 32 && x <= size * 20 / 32)
                && (y >= size * 6 / 32 && y <= size * 20 / 32);
            let microphone_base = (x >= size * 9 / 32 && x <= size * 22 / 32)
                && (y >= size * 18 / 32 && y <= size * 21 / 32);
            let stem = (x >= size * 15 / 32 && x <= size * 17 / 32)
                && (y >= size * 20 / 32 && y <= size * 25 / 32);
            let foot = (x >= size * 11 / 32 && x <= size * 21 / 32)
                && (y >= size * 24 / 32 && y <= size * 26 / 32);
            if microphone || microphone_base || stem || foot {
                rgba[offset..offset + 4].copy_from_slice(&[255, 255, 255, 255]);
            }
        }
    }

    rgba
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tray_labels_follow_voice_lifecycle() {
        let idle = tray_labels(TrayPhase::Idle);
        assert_eq!(idle.toggle, "开始语音输入");
        assert!(idle.can_toggle);

        let activating = tray_labels(TrayPhase::Activating);
        assert_eq!(activating.toggle, "取消语音输入");
        assert!(activating.can_toggle);

        let listening = tray_labels(TrayPhase::Listening);
        assert_eq!(listening.toggle, "结束语音输入");
        assert!(listening.can_toggle);

        let finishing = tray_labels(TrayPhase::Finishing);
        assert!(!finishing.can_toggle);
    }

    #[test]
    fn generated_icon_has_the_expected_rgba_shape() {
        let pixels = tray_icon_rgba(TRAY_ICON_SIZE);
        assert_eq!(pixels.len(), (TRAY_ICON_SIZE * TRAY_ICON_SIZE * 4) as usize);
        assert!(pixels.chunks_exact(4).any(|pixel| pixel[3] != 0));
    }
}
