#[cfg(target_os = "windows")]
mod implementation {
    use std::ffi::c_void;
    use std::sync::Mutex;
    use std::thread;
    use std::time::Duration;

    use unicode_segmentation::UnicodeSegmentation as _;
    use windows::Win32::Foundation::{GlobalFree, HANDLE, HWND};
    use windows::Win32::System::DataExchange::{
        CloseClipboard, EmptyClipboard, OpenClipboard, SetClipboardData,
    };
    use windows::Win32::System::Memory::{GMEM_MOVEABLE, GlobalAlloc, GlobalLock, GlobalUnlock};
    use windows::Win32::UI::Input::KeyboardAndMouse::{
        GetAsyncKeyState, INPUT, INPUT_0, INPUT_KEYBOARD, KEYBDINPUT, KEYEVENTF_KEYUP,
        KEYEVENTF_UNICODE, SendInput, VIRTUAL_KEY, VK_BACK, VK_CONTROL, VK_LWIN, VK_MENU, VK_RWIN,
        VK_SHIFT,
    };
    use windows::Win32::UI::WindowsAndMessaging::GetForegroundWindow;

    const CF_UNICODETEXT: u32 = 13;

    #[derive(Clone, Copy, Debug, Default)]
    pub struct PasteTarget {
        clipboard_owner: isize,
        input_target: isize,
    }

    impl PasteTarget {
        pub fn for_window(clipboard_owner: HWND) -> Self {
            Self {
                clipboard_owner: clipboard_owner.0 as isize,
                input_target: unsafe { GetForegroundWindow() }.0 as isize,
            }
        }
    }

    #[derive(Debug, Default)]
    struct RealtimeTextState {
        current_text: String,
        desynchronized: bool,
    }

    #[derive(Debug)]
    pub struct RealtimeTextOutput {
        target: PasteTarget,
        state: Mutex<RealtimeTextState>,
    }

    impl RealtimeTextOutput {
        pub fn new(target: PasteTarget) -> Self {
            Self {
                target,
                state: Mutex::new(RealtimeTextState::default()),
            }
        }

        pub fn update(&self, text: &str) -> Result<(), String> {
            if self.target.input_target == 0 {
                return Err("no foreground application was available for live text".to_string());
            }
            let foreground = unsafe { GetForegroundWindow() };
            if foreground.0 as isize != self.target.input_target {
                return Err(
                    "the application receiving voice input lost focus; live text was paused"
                        .to_string(),
                );
            }
            if modifier_key_is_pressed() {
                return Err("a keyboard modifier is held; live text was paused".to_string());
            }
            let mut state = self
                .state
                .lock()
                .map_err(|_| "live text state lock is poisoned".to_string())?;
            if state.desynchronized {
                return Err(
                    "live text stopped after Windows accepted only part of an earlier update"
                        .to_string(),
                );
            }
            if state.current_text == text {
                return Ok(());
            }
            let replacement = plan_replacement(&state.current_text, text)?;
            if let Err(error) = send_replacement(replacement) {
                state.desynchronized = true;
                return Err(error);
            }
            state.current_text.clear();
            state.current_text.push_str(text);
            Ok(())
        }

        pub fn finish(&self, text: &str) -> Result<(), String> {
            if let Err(error) = self.update(text) {
                set_clipboard_text(text, self.target).map_err(|clipboard_error| {
                    format!("{error}; could not copy the final text either: {clipboard_error}")
                })?;
                return Err(format!("{error}; the final text is on the clipboard"));
            }
            Ok(())
        }
    }

    fn modifier_key_is_pressed() -> bool {
        [VK_CONTROL, VK_MENU, VK_SHIFT, VK_LWIN, VK_RWIN]
            .into_iter()
            .any(|key| unsafe { GetAsyncKeyState(key.0 as i32) } < 0)
    }

    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    struct TextReplacement<'a> {
        erase_keypresses: usize,
        insert: &'a str,
    }

    fn plan_replacement<'a>(current: &str, next: &'a str) -> Result<TextReplacement<'a>, String> {
        let mut common_prefix_bytes = 0;
        for (current_grapheme, next_grapheme) in current.graphemes(true).zip(next.graphemes(true)) {
            if current_grapheme != next_grapheme {
                break;
            }
            common_prefix_bytes += current_grapheme.len();
        }

        let obsolete_suffix = &current[common_prefix_bytes..];
        // Text controls disagree on whether Backspace consumes a grapheme,
        // scalar value, or UTF-16 unit. Only emit deletion input when all
        // three counts are identical, and validate before sending any key.
        if obsolete_suffix
            .graphemes(true)
            .any(|grapheme| grapheme.encode_utf16().count() != 1)
        {
            return Err(
                "the changed live-text suffix contains Unicode that this Windows control may not erase atomically"
                    .to_string(),
            );
        }

        Ok(TextReplacement {
            erase_keypresses: obsolete_suffix.encode_utf16().count(),
            insert: &next[common_prefix_bytes..],
        })
    }

    fn send_replacement(replacement: TextReplacement<'_>) -> Result<(), String> {
        let utf16_length = replacement.insert.encode_utf16().count();
        let mut inputs = Vec::with_capacity((replacement.erase_keypresses + utf16_length) * 2);
        for _ in 0..replacement.erase_keypresses {
            inputs.push(keyboard_input(VK_BACK, false));
            inputs.push(keyboard_input(VK_BACK, true));
        }
        for code_unit in replacement.insert.encode_utf16() {
            inputs.push(unicode_input(code_unit, false));
            inputs.push(unicode_input(code_unit, true));
        }
        if inputs.is_empty() {
            return Ok(());
        }
        let sent = unsafe { SendInput(&inputs, std::mem::size_of::<INPUT>() as i32) };
        if sent != inputs.len() as u32 {
            return Err(format!(
                "Windows accepted only {sent} of {} live text input events",
                inputs.len()
            ));
        }
        Ok(())
    }

    fn keyboard_input(key: VIRTUAL_KEY, key_up: bool) -> INPUT {
        INPUT {
            r#type: INPUT_KEYBOARD,
            Anonymous: INPUT_0 {
                ki: KEYBDINPUT {
                    wVk: key,
                    dwFlags: if key_up {
                        KEYEVENTF_KEYUP
                    } else {
                        Default::default()
                    },
                    ..Default::default()
                },
            },
        }
    }

    fn unicode_input(code_unit: u16, key_up: bool) -> INPUT {
        INPUT {
            r#type: INPUT_KEYBOARD,
            Anonymous: INPUT_0 {
                ki: KEYBDINPUT {
                    wScan: code_unit,
                    dwFlags: if key_up {
                        KEYEVENTF_UNICODE | KEYEVENTF_KEYUP
                    } else {
                        KEYEVENTF_UNICODE
                    },
                    ..Default::default()
                },
            },
        }
    }

    fn set_clipboard_text(text: &str, target: PasteTarget) -> Result<(), String> {
        let utf16: Vec<u16> = text.encode_utf16().chain(std::iter::once(0)).collect();
        unsafe {
            let owner = Some(HWND(target.clipboard_owner as *mut c_void));
            let mut clipboard_open = false;
            for _ in 0..10 {
                if OpenClipboard(owner).is_ok() {
                    clipboard_open = true;
                    break;
                }
                thread::sleep(Duration::from_millis(10));
            }
            if !clipboard_open {
                return Err("could not open clipboard after 10 attempts".to_string());
            }
            let result = (|| {
                EmptyClipboard().map_err(|error| format!("could not clear clipboard: {error}"))?;
                let memory =
                    GlobalAlloc(GMEM_MOVEABLE, utf16.len() * std::mem::size_of::<u16>())
                        .map_err(|error| format!("could not allocate clipboard memory: {error}"))?;
                let pointer = GlobalLock(memory);
                if pointer.is_null() {
                    let _ = GlobalFree(Some(memory));
                    return Err("could not lock clipboard memory".to_string());
                }
                std::ptr::copy_nonoverlapping(utf16.as_ptr(), pointer.cast::<u16>(), utf16.len());
                let _ = GlobalUnlock(memory);
                if SetClipboardData(CF_UNICODETEXT, Some(HANDLE(memory.0))).is_err() {
                    let _ = GlobalFree(Some(memory));
                    return Err("could not set clipboard text".to_string());
                }
                Ok(())
            })();
            let _ = CloseClipboard();
            result
        }
    }

    #[cfg(test)]
    mod tests {
        use super::{TextReplacement, plan_replacement};

        #[test]
        fn growing_snapshot_only_appends_the_new_suffix() {
            assert_eq!(
                plan_replacement("你好", "你好世界").unwrap(),
                TextReplacement {
                    erase_keypresses: 0,
                    insert: "世界",
                }
            );
        }

        #[test]
        fn revised_snapshot_replaces_only_the_changed_tail() {
            assert_eq!(
                plan_replacement("今天天气晴", "今天天气很好").unwrap(),
                TextReplacement {
                    erase_keypresses: 1,
                    insert: "很好",
                }
            );
        }

        #[test]
        fn joined_emoji_revision_is_rejected_before_input() {
            assert!(plan_replacement("家庭👨‍👩‍👧", "家庭🙂").is_err());
        }

        #[test]
        fn combining_sequence_revision_is_rejected_before_input() {
            assert!(plan_replacement("cafe\u{301}", "cafe").is_err());
        }

        #[test]
        fn shorter_snapshot_erases_the_obsolete_suffix() {
            assert_eq!(
                plan_replacement("hello world", "hello").unwrap(),
                TextReplacement {
                    erase_keypresses: 6,
                    insert: "",
                }
            );
        }
    }
}

#[cfg(target_os = "linux")]
mod implementation {
    /// Linux delivers text through fcitx5, which already owns the input focus
    /// of every application on the desktop, so there is no per-window target to
    /// capture the way Windows captures a foreground `HWND`.
    #[derive(Clone, Copy, Debug, Default)]
    pub struct PasteTarget;

    /// The `local.doubao.VoiceBridge1` object the Doubao fcitx5 addon publishes
    /// on fcitx5's own bus connection. `clients/fcitx5-addon` builds it.
    #[zbus::proxy(
        interface = "local.doubao.VoiceBridge1",
        default_service = "org.fcitx.Fcitx5",
        default_path = "/voicebridge"
    )]
    trait VoiceBridge {
        /// Commits `text` into the focused application. `false` means nothing
        /// was focused, so the text was not delivered anywhere.
        fn commit_string(&self, text: &str) -> zbus::Result<bool>;
    }

    #[derive(Debug)]
    pub struct RealtimeTextOutput {
        bridge: Result<VoiceBridgeProxyBlocking<'static>, String>,
    }

    impl RealtimeTextOutput {
        pub fn new(_target: PasteTarget) -> Self {
            Self { bridge: connect() }
        }

        /// Partial recognition is not written to the focused application yet.
        /// Showing it there as provisional preedit lands with the next layer.
        pub fn update(&self, _text: &str) -> Result<(), String> {
            Ok(())
        }

        pub fn finish(&self, text: &str) -> Result<(), String> {
            let delivered = self
                .bridge
                .as_ref()
                .map_err(String::clone)?
                .commit_string(text)
                .map_err(|error| {
                    format!(
                        "could not hand the text to fcitx5; build and install the addon from \
                         clients/fcitx5-addon, then restart fcitx5 ({error})"
                    )
                })?;
            if !delivered {
                return Err(
                    "no application held the input focus, so the recognized text was not delivered"
                        .to_string(),
                );
            }
            Ok(())
        }
    }

    fn connect() -> Result<VoiceBridgeProxyBlocking<'static>, String> {
        let connection = zbus::blocking::Connection::session()
            .map_err(|error| format!("could not reach the session bus: {error}"))?;
        VoiceBridgeProxyBlocking::new(&connection)
            .map_err(|error| format!("could not address the fcitx5 voice bridge: {error}"))
    }
}

pub use implementation::{PasteTarget, RealtimeTextOutput};
