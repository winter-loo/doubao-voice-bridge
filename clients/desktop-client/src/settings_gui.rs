use gpui::{
    App, Application, ClickEvent, Context, Entity, FocusHandle, Focusable, FontWeight, KeyBinding,
    KeyDownEvent, KeyUpEvent, Modifiers, ModifiersChangedEvent, Render, Window,
    WindowBackgroundAppearance, WindowBounds, WindowKind, WindowOptions, div, prelude::*, px, rgb,
    rgba, size,
};

use crate::client_settings::ClientSettings;
use crate::linux_shortcut;
use crate::linux_text_input::{self, TextInput};
use crate::native_voice;

struct SettingsWindow {
    server: Entity<TextInput>,
    port: Entity<TextInput>,
    shortcut: Entity<ShortcutCapture>,
    status: String,
    connection_tests: ConnectionTestTracker,
}

#[derive(Default)]
struct ConnectionTestTracker {
    generation: u64,
}

impl ConnectionTestTracker {
    fn begin(&mut self) -> u64 {
        self.generation = self.generation.wrapping_add(1);
        self.generation
    }

    fn invalidate(&mut self) {
        self.generation = self.generation.wrapping_add(1);
    }

    fn is_current(&self, generation: u64) -> bool {
        self.generation == generation
    }
}

struct ShortcutCapture {
    committed: String,
    modifiers: Modifiers,
    pressed_key: Option<String>,
    capturing: bool,
    session_has_key: bool,
    focus_handle: FocusHandle,
}

impl ShortcutCapture {
    fn new(value: String, cx: &mut Context<Self>) -> Self {
        Self {
            committed: value,
            modifiers: Modifiers::default(),
            pressed_key: None,
            capturing: false,
            session_has_key: false,
            focus_handle: cx.focus_handle(),
        }
    }

    fn capture(&mut self, event: &KeyDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        if !self.focus_handle.is_focused(window) || !self.capturing {
            return;
        }
        if event.is_held {
            return;
        }
        let Some(key) = normal_shortcut_key(&event.keystroke.key) else {
            return;
        };
        self.modifiers = event.keystroke.modifiers;
        self.pressed_key = Some(key);
        self.committed = self.live_value();
        self.session_has_key = true;
        cx.notify();
    }

    fn release(&mut self, event: &KeyUpEvent, window: &mut Window, cx: &mut Context<Self>) {
        if !self.focus_handle.is_focused(window) || !self.capturing {
            return;
        }
        if let Some(key) = normal_shortcut_key(&event.keystroke.key)
            && self.pressed_key.as_deref() == Some(key.as_str())
        {
            // The first release completes the chord. Keep the last full live
            // value visible so the user can confirm what will be saved.
            self.capturing = false;
            cx.notify();
        }
    }

    fn modifiers_changed(
        &mut self,
        event: &ModifiersChangedEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.focus_handle.is_focused(window) || !self.capturing {
            return;
        }
        let previous = self.modifiers;
        let modifier_released = (previous.control && !event.modifiers.control)
            || (previous.alt && !event.modifiers.alt)
            || (previous.shift && !event.modifiers.shift)
            || (previous.platform && !event.modifiers.platform);
        if self.session_has_key && modifier_released {
            self.capturing = false;
            cx.notify();
            return;
        }
        self.modifiers = event.modifiers;
        if self.session_has_key {
            self.committed = self.live_value();
        }
        cx.notify();
    }

    fn focus(&mut self, _: &ClickEvent, window: &mut Window, cx: &mut Context<Self>) {
        window.focus(&self.focus_handle);
        self.modifiers = Modifiers::default();
        self.pressed_key = None;
        self.capturing = true;
        self.session_has_key = false;
        cx.notify();
    }

    fn live_value(&self) -> String {
        let mut parts = Vec::new();
        if self.modifiers.control {
            parts.push("CTRL".to_string());
        }
        if self.modifiers.alt {
            parts.push("ALT".to_string());
        }
        if self.modifiers.shift {
            parts.push("SHIFT".to_string());
        }
        if self.modifiers.platform {
            parts.push("LOGO".to_string());
        }
        if let Some(key) = &self.pressed_key {
            parts.push(key.clone());
        }
        parts.join("+")
    }
}

impl Render for ShortcutCapture {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .id("shortcut-capture")
            .size_full()
            .key_context("ShortcutCapture")
            .track_focus(&self.focus_handle)
            .cursor(gpui::CursorStyle::IBeam)
            .on_click(cx.listener(Self::focus))
            .on_key_down(cx.listener(Self::capture))
            .on_key_up(cx.listener(Self::release))
            .on_modifiers_changed(cx.listener(Self::modifiers_changed))
            .text_sm()
            .child(if self.capturing && self.live_value().is_empty() {
                "请按住并输入快捷键…".to_string()
            } else if self.capturing {
                self.live_value()
            } else {
                self.committed.clone()
            })
    }
}

fn normal_shortcut_key(raw: &str) -> Option<String> {
    let key = raw.to_ascii_lowercase();
    if matches!(
        key.as_str(),
        "control" | "ctrl" | "alt" | "shift" | "super" | "logo" | "meta"
    ) {
        return None;
    }
    key.bytes()
        .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_')
        .then_some(key)
}

fn parse_audio_port(value: &str) -> Result<u16, &'static str> {
    value
        .parse::<u16>()
        .ok()
        .filter(|port| *port != 0)
        .ok_or("音频端口必须是 1–65535")
}

fn connection_status(server: &str, result: Result<(), String>) -> String {
    match result {
        Ok(()) => format!("连接成功：{server}"),
        Err(error) => format!("连接失败：{error}"),
    }
}

impl Focusable for ShortcutCapture {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl SettingsWindow {
    fn test_connection(&mut self, cx: &mut Context<Self>) {
        let server = self.server.read(cx).content().trim().to_string();
        let generation = self.connection_tests.begin();
        self.status = "正在测试连接…".to_string();
        cx.notify();

        let connection_test = cx.background_spawn(async move {
            let result = native_voice::test_connection(&server);
            (server, result)
        });
        cx.spawn(async move |this, cx| {
            let (server, result) = connection_test.await;
            this.update(cx, |this, cx| {
                if !this.connection_tests.is_current(generation) {
                    return;
                }
                this.status = connection_status(&server, result);
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    fn save(&mut self, cx: &mut Context<Self>) {
        self.connection_tests.invalidate();
        let server = self.server.read(cx).content().trim().to_string();
        let port_text = self.port.read(cx).content().trim().to_string();
        let shortcut = self.shortcut.read(cx).committed.trim().to_string();
        let mut settings = ClientSettings::load().unwrap_or_default();
        let audio_port = parse_audio_port(&port_text);
        if server.is_empty() {
            self.status = "服务器地址不能为空".to_string();
        } else if let Err(error) = audio_port {
            self.status = error.to_string();
        } else if linux_shortcut::LinuxShortcut::parse(&shortcut).is_err() {
            self.status = "快捷键格式无效，例如 CTRL+ALT+v".to_string();
        } else {
            settings.server = server;
            settings.audio_port = audio_port.expect("audio port was validated");
            settings.voice_shortcut = shortcut;
            settings.setup_completed = true;
            self.status = match settings.save() {
                Ok(()) => "已保存。新的快捷键将在重启客户端后生效。".to_string(),
                Err(error) => format!("保存失败：{error}"),
            };
        }
        cx.notify();
    }
}

impl Render for SettingsWindow {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let save = cx.listener(|this, _, _, cx| this.save(cx));
        div()
            .size_full()
            .flex()
            .flex_col()
            .gap_4()
            .p_6()
            .bg(rgb(0x0b1120))
            .text_color(rgba(0xfffffff2))
            .child(
                div()
                    .text_xl()
                    .font_weight(FontWeight::SEMIBOLD)
                    .child("豆包语音客户端"),
            )
            .child(
                div()
                    .text_sm()
                    .text_color(rgba(0xb7c4d9ff))
                    .child("配置语音桥接服务和 Linux 快捷键"),
            )
            .child(field_with_action(
                "桥接服务器",
                self.server.clone(),
                "测试连接",
                cx.listener(|this, _, _, cx| this.test_connection(cx)),
            ))
            .child(field("音频端口", self.port.clone()))
            .child(shortcut_field(self.shortcut.clone()))
            .child(
                div()
                    .text_xs()
                    .text_color(rgba(0x8fa3bfff))
                    .child("示例：F13、CTRL+ALT+v。环境变量会覆盖这里的设置。"),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .child(
                        div()
                            .text_xs()
                            .text_color(rgba(0x7dd3fcff))
                            .child(self.status.clone()),
                    )
                    .child(
                        div()
                            .id("save-settings")
                            .px_4()
                            .py_2()
                            .rounded_md()
                            .bg(rgb(0x2563eb))
                            .hover(|style| style.bg(rgb(0x3b82f6)))
                            .cursor_pointer()
                            .on_click(save)
                            .child("保存设置"),
                    ),
            )
    }
}

fn field(label: &'static str, input: Entity<TextInput>) -> impl IntoElement {
    div()
        .flex()
        .flex_col()
        .gap_2()
        .child(div().text_sm().child(label))
        .child(
            div()
                .h(px(42.0))
                .w_full()
                .px_3()
                .py_2()
                .rounded_md()
                .bg(rgb(0x111827))
                .border_1()
                .border_color(rgba(0xffffff24))
                .child(input),
        )
}

fn field_with_action(
    label: &'static str,
    input: Entity<TextInput>,
    action: &'static str,
    on_click: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
) -> impl IntoElement {
    div()
        .flex()
        .flex_col()
        .gap_2()
        .child(
            div()
                .flex()
                .items_center()
                .justify_between()
                .child(div().text_sm().child(label))
                .child(
                    div()
                        .id("test-connection")
                        .px_2()
                        .py_1()
                        .rounded_md()
                        .text_xs()
                        .bg(rgba(0x2563eb33))
                        .text_color(rgba(0x93c5fdff))
                        .cursor_pointer()
                        .on_click(on_click)
                        .child(action),
                ),
        )
        .child(
            div()
                .h(px(42.0))
                .w_full()
                .px_3()
                .py_2()
                .rounded_md()
                .bg(rgb(0x111827))
                .border_1()
                .border_color(rgba(0xffffff24))
                .child(input),
        )
}

fn shortcut_field(input: Entity<ShortcutCapture>) -> impl IntoElement {
    div()
        .flex()
        .flex_col()
        .gap_2()
        .child(div().text_sm().child("语音快捷键"))
        .child(
            div()
                .h(px(42.0))
                .w_full()
                .px_3()
                .py_2()
                .rounded_md()
                .bg(rgb(0x111827))
                .border_1()
                .border_color(rgba(0xffffff24))
                .child(input),
        )
}

pub fn run() {
    Application::new().run(|cx: &mut App| {
        cx.bind_keys([
            KeyBinding::new(
                "backspace",
                linux_text_input::Backspace,
                Some("TextInput"),
            ),
            KeyBinding::new(
                "delete",
                linux_text_input::Delete,
                Some("TextInput"),
            ),
            KeyBinding::new(
                "left",
                linux_text_input::Left,
                Some("TextInput"),
            ),
            KeyBinding::new(
                "right",
                linux_text_input::Right,
                Some("TextInput"),
            ),
            KeyBinding::new(
                "shift-left",
                linux_text_input::SelectLeft,
                Some("TextInput"),
            ),
            KeyBinding::new(
                "shift-right",
                linux_text_input::SelectRight,
                Some("TextInput"),
            ),
            KeyBinding::new(
                "cmd-a",
                linux_text_input::SelectAll,
                Some("TextInput"),
            ),
            KeyBinding::new(
                "cmd-v",
                linux_text_input::Paste,
                Some("TextInput"),
            ),
            KeyBinding::new(
                "cmd-c",
                linux_text_input::Copy,
                Some("TextInput"),
            ),
            KeyBinding::new(
                "home",
                linux_text_input::Home,
                Some("TextInput"),
            ),
            KeyBinding::new("end", linux_text_input::End, Some("TextInput")),
        ]);
        let settings = ClientSettings::load().unwrap_or_default();
        let server = cx.new(|cx| TextInput::new(cx));
        let port = cx.new(|cx| TextInput::new(cx));
        let shortcut = cx.new(|cx| ShortcutCapture::new(settings.voice_shortcut.clone(), cx));
        server.update(cx, |input, _| input.set_text(&settings.server));
        port.update(cx, |input, _| {
            input.set_text(&settings.audio_port.to_string())
        });
        let options = WindowOptions {
            window_bounds: Some(WindowBounds::Windowed(gpui::Bounds::centered(
                None,
                // Leave enough room for the titlebar, three 42px editors, and
                // the action row.  GPUI's logical pixels are scaled on HiDPI
                // Linux displays, so the previous 430px window clipped the
                // save button below the viewport.
                size(px(520.0), px(520.0)),
                cx,
            ))),
            titlebar: Some(gpui::TitlebarOptions {
                title: Some("Doubao Voice Client".into()),
                ..Default::default()
            }),
            kind: WindowKind::Normal,
            window_background: WindowBackgroundAppearance::Opaque,
            ..Default::default()
        };
        cx.open_window(options, |_, cx| {
            cx.new(|_| SettingsWindow {
                server,
                port,
                shortcut,
                status: String::new(),
                connection_tests: ConnectionTestTracker::default(),
            })
        })
        .expect("failed to open settings window");
    });
}

#[cfg(test)]
mod tests {
    use super::{ConnectionTestTracker, connection_status, parse_audio_port};

    #[test]
    fn audio_port_zero_is_rejected() {
        assert_eq!(parse_audio_port("0"), Err("音频端口必须是 1–65535"));
        assert_eq!(parse_audio_port("1"), Ok(1));
        assert_eq!(parse_audio_port("65535"), Ok(65_535));
    }

    #[test]
    fn connection_result_is_formatted_after_the_worker_finishes() {
        assert_eq!(
            connection_status("bridge.local:5003", Ok(())),
            "连接成功：bridge.local:5003"
        );
        assert_eq!(
            connection_status("bridge.local:5003", Err("timed out".to_string())),
            "连接失败：timed out"
        );
    }

    #[test]
    fn only_the_latest_connection_test_can_publish_its_result() {
        let mut tests = ConnectionTestTracker::default();

        let first = tests.begin();
        let second = tests.begin();
        assert!(!tests.is_current(first));
        assert!(tests.is_current(second));

        tests.invalidate();
        assert!(!tests.is_current(second));
    }
}
