use std::sync::{Arc, Mutex, OnceLock};

use ksni::{
    Icon, MenuItem, ToolTip, Tray,
    blocking::{Handle, TrayMethods as _},
    menu::StandardItem,
};

use crate::tray_icon::{VOICE_T_TRAY_ICON_SIZE, voice_t_tray_icon_argb};

static TRAY_HANDLE: OnceLock<Mutex<Option<Handle<VoiceTray>>>> = OnceLock::new();

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TrayPhase {
    Idle,
    Activating,
    Listening,
    Finishing,
}

struct VoiceTray {
    phase: fn() -> TrayPhase,
    on_toggle: Arc<dyn Fn() + Send + Sync>,
    on_settings: Arc<dyn Fn() + Send + Sync>,
    on_quit: Arc<dyn Fn() + Send + Sync>,
}

impl Tray for VoiceTray {
    const MENU_ON_ACTIVATE: bool = true;

    fn id(&self) -> String {
        "doubao-voice-client".to_string()
    }

    fn title(&self) -> String {
        "Doubao Voice Client".to_string()
    }

    fn icon_pixmap(&self) -> Vec<Icon> {
        vec![Icon {
            width: VOICE_T_TRAY_ICON_SIZE as i32,
            height: VOICE_T_TRAY_ICON_SIZE as i32,
            data: voice_t_tray_icon_argb(),
        }]
    }

    fn tool_tip(&self) -> ToolTip {
        ToolTip {
            title: self.title(),
            description: tray_labels((self.phase)()).status.to_string(),
            icon_pixmap: self.icon_pixmap(),
            ..Default::default()
        }
    }

    fn menu(&self) -> Vec<MenuItem<Self>> {
        let labels = tray_labels((self.phase)());
        let on_toggle = Arc::clone(&self.on_toggle);
        let on_settings = Arc::clone(&self.on_settings);
        let on_quit = Arc::clone(&self.on_quit);

        vec![
            StandardItem {
                label: labels.status.to_string(),
                enabled: false,
                ..Default::default()
            }
            .into(),
            StandardItem {
                label: labels.toggle.to_string(),
                enabled: labels.can_toggle,
                activate: Box::new(move |_| on_toggle()),
                ..Default::default()
            }
            .into(),
            StandardItem {
                label: "打开设置".to_string(),
                icon_name: "preferences-system".to_string(),
                activate: Box::new(move |_| on_settings()),
                ..Default::default()
            }
            .into(),
            MenuItem::Separator,
            StandardItem {
                label: "退出".to_string(),
                icon_name: "application-exit".to_string(),
                activate: Box::new(move |_| on_quit()),
                ..Default::default()
            }
            .into(),
        ]
    }
}

pub fn start(
    phase: fn() -> TrayPhase,
    on_toggle: impl Fn() + Send + Sync + 'static,
    on_settings: impl Fn() + Send + Sync + 'static,
    on_quit: impl Fn() + Send + Sync + 'static,
) -> Result<(), String> {
    let tray = VoiceTray {
        phase,
        on_toggle: Arc::new(on_toggle),
        on_settings: Arc::new(on_settings),
        on_quit: Arc::new(on_quit),
    };
    let handle = tray
        .spawn()
        .map_err(|error| format!("could not register StatusNotifierItem: {error}"))?;

    let slot = TRAY_HANDLE.get_or_init(|| Mutex::new(None));
    *slot
        .lock()
        .map_err(|_| "system tray handle lock is poisoned".to_string())? = Some(handle);
    Ok(())
}

/// Tell StatusNotifier hosts that the dynamic labels and enabled state changed.
pub fn refresh() {
    if TRAY_HANDLE.get().is_none() {
        return;
    }
    // A phase change can originate inside a tray activation callback. Updating
    // synchronously there would wait on the same service thread.
    std::thread::spawn(|| {
        let Some(slot) = TRAY_HANDLE.get() else {
            return;
        };
        let Ok(handle) = slot.lock() else {
            return;
        };
        if let Some(handle) = handle.as_ref() {
            let _ = handle.update(|_| {});
        }
    });
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
    fn tray_labels_return_to_idle_between_repeated_sessions() {
        let phases = [
            TrayPhase::Activating,
            TrayPhase::Finishing,
            TrayPhase::Idle,
            TrayPhase::Activating,
            TrayPhase::Listening,
            TrayPhase::Finishing,
            TrayPhase::Idle,
        ];
        let toggles: Vec<_> = phases
            .into_iter()
            .map(|phase| tray_labels(phase).toggle)
            .collect();

        assert_eq!(toggles[2], "开始语音输入");
        assert_eq!(toggles[3], "取消语音输入");
        assert_eq!(toggles[6], "开始语音输入");
    }
}
