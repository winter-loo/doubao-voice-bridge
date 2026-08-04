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
            let replacement = plan_replacement(&state.current_text, text);
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
        erase_graphemes: usize,
        insert: &'a str,
    }

    fn plan_replacement<'a>(current: &str, next: &'a str) -> TextReplacement<'a> {
        let mut common_prefix_bytes = 0;
        for (current_grapheme, next_grapheme) in current.graphemes(true).zip(next.graphemes(true)) {
            if current_grapheme != next_grapheme {
                break;
            }
            common_prefix_bytes += current_grapheme.len();
        }
        TextReplacement {
            erase_graphemes: current[common_prefix_bytes..].graphemes(true).count(),
            insert: &next[common_prefix_bytes..],
        }
    }

    fn send_replacement(replacement: TextReplacement<'_>) -> Result<(), String> {
        let utf16_length = replacement.insert.encode_utf16().count();
        let mut inputs = Vec::with_capacity((replacement.erase_graphemes + utf16_length) * 2);
        for _ in 0..replacement.erase_graphemes {
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
                plan_replacement("你好", "你好世界"),
                TextReplacement {
                    erase_graphemes: 0,
                    insert: "世界",
                }
            );
        }

        #[test]
        fn revised_snapshot_replaces_only_the_changed_tail() {
            assert_eq!(
                plan_replacement("今天天气晴", "今天天气很好"),
                TextReplacement {
                    erase_graphemes: 1,
                    insert: "很好",
                }
            );
        }

        #[test]
        fn replacement_counts_joined_emoji_as_one_cursor_step() {
            assert_eq!(
                plan_replacement("家庭👨‍👩‍👧", "家庭🙂"),
                TextReplacement {
                    erase_graphemes: 1,
                    insert: "🙂",
                }
            );
        }

        #[test]
        fn shorter_snapshot_erases_the_obsolete_suffix() {
            assert_eq!(
                plan_replacement("hello world", "hello"),
                TextReplacement {
                    erase_graphemes: 6,
                    insert: "",
                }
            );
        }
    }
}

#[cfg(target_os = "linux")]
mod implementation {
    use std::io::Write as _;
    use std::os::unix::fs::PermissionsExt as _;
    use std::path::PathBuf;
    use std::process::{Command, Stdio};

    #[derive(Clone, Copy, Debug, Default)]
    pub struct PasteTarget;

    #[derive(Debug, Default)]
    pub struct RealtimeTextOutput;

    impl RealtimeTextOutput {
        pub fn new(_target: PasteTarget) -> Self {
            Self
        }

        pub fn update(&self, _text: &str) -> Result<(), String> {
            Ok(())
        }

        pub fn finish(&self, _text: &str) -> Result<(), String> {
            Ok(())
        }
    }

    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    enum LinuxClipboardTool {
        WlCopy,
        Xclip,
        Xsel,
    }

    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    enum LinuxPasteTool {
        Ydotool,
        Xdotool,
    }

    fn choose_clipboard_tool<F>(wayland: bool, mut available: F) -> Option<LinuxClipboardTool>
    where
        F: FnMut(&str) -> bool,
    {
        if wayland && available("wl-copy") {
            return Some(LinuxClipboardTool::WlCopy);
        }
        if available("xclip") {
            return Some(LinuxClipboardTool::Xclip);
        }
        if available("xsel") {
            return Some(LinuxClipboardTool::Xsel);
        }
        None
    }

    fn choose_paste_tool<F>(mut available: F) -> Option<LinuxPasteTool>
    where
        F: FnMut(&str) -> bool,
    {
        if available("ydotool") {
            return Some(LinuxPasteTool::Ydotool);
        }
        if available("xdotool") {
            return Some(LinuxPasteTool::Xdotool);
        }
        None
    }

    fn command_exists(command: &str) -> bool {
        let Some(path) = std::env::var_os("PATH") else {
            return false;
        };
        std::env::split_paths(&path).any(|directory| {
            let candidate: PathBuf = directory.join(command);
            candidate.metadata().is_ok_and(|metadata| {
                metadata.is_file() && metadata.permissions().mode() & 0o111 != 0
            })
        })
    }

    fn write_clipboard(command: &str, args: &[&str], text: &str) -> Result<(), String> {
        let mut child = Command::new(command)
            .args(args)
            .stdin(Stdio::piped())
            .spawn()
            .map_err(|error| format!("could not start {command}: {error}"))?;
        child
            .stdin
            .take()
            .ok_or_else(|| format!("could not open {command} stdin"))?
            .write_all(text.as_bytes())
            .map_err(|error| format!("could not send text to {command}: {error}"))?;
        let status = child
            .wait()
            .map_err(|error| format!("could not wait for {command}: {error}"))?;
        if status.success() {
            Ok(())
        } else {
            Err(format!("{command} exited with {status}"))
        }
    }

    #[allow(dead_code)]
    pub fn paste_text(text: &str, _target: PasteTarget) -> Result<(), String> {
        let wayland = std::env::var_os("WAYLAND_DISPLAY").is_some();
        match choose_clipboard_tool(wayland, command_exists) {
            Some(LinuxClipboardTool::WlCopy) => write_clipboard("wl-copy", &[], text)?,
            Some(LinuxClipboardTool::Xclip) => {
                write_clipboard("xclip", &["-selection", "clipboard"], text)?
            }
            Some(LinuxClipboardTool::Xsel) => {
                write_clipboard("xsel", &["--clipboard", "--input"], text)?
            }
            None => {
                return Err(
                    "install wl-copy, xclip, or xsel to receive recognized text".to_string()
                );
            }
        }

        let (command, args): (&str, &[&str]) = match choose_paste_tool(command_exists) {
            Some(LinuxPasteTool::Ydotool) => ("ydotool", &["key", "29:1", "47:1", "47:0", "29:0"]),
            Some(LinuxPasteTool::Xdotool) => ("xdotool", &["key", "--clearmodifiers", "ctrl+v"]),
            None => {
                return Err(
                    "recognized text is on the clipboard; install ydotool or xdotool to paste it"
                        .to_string(),
                );
            }
        };
        let status = Command::new(command)
            .args(args)
            .status()
            .map_err(|error| format!("could not start {command}: {error}"))?;
        if status.success() {
            Ok(())
        } else {
            Err(format!(
                "recognized text is on the clipboard, but {command} exited with {status}"
            ))
        }
    }

    #[cfg(test)]
    mod tests {
        use super::{LinuxClipboardTool, LinuxPasteTool, choose_clipboard_tool, choose_paste_tool};

        #[test]
        fn prefers_session_native_clipboard_then_x11_fallbacks() {
            assert_eq!(
                choose_clipboard_tool(true, |tool| matches!(tool, "wl-copy" | "xclip")),
                Some(LinuxClipboardTool::WlCopy)
            );
            assert_eq!(
                choose_clipboard_tool(false, |tool| matches!(tool, "wl-copy" | "xclip")),
                Some(LinuxClipboardTool::Xclip)
            );
            assert_eq!(
                choose_clipboard_tool(false, |tool| tool == "xsel"),
                Some(LinuxClipboardTool::Xsel)
            );
        }

        #[test]
        fn prefers_wayland_capable_global_input_injector() {
            assert_eq!(
                choose_paste_tool(|tool| matches!(tool, "ydotool" | "xdotool")),
                Some(LinuxPasteTool::Ydotool)
            );
            assert_eq!(
                choose_paste_tool(|tool| tool == "xdotool"),
                Some(LinuxPasteTool::Xdotool)
            );
        }
    }
}

pub use implementation::{PasteTarget, RealtimeTextOutput};
