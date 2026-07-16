use std::time::Duration;

use gpui::{
    Animation, AnimationExt as _, App, Application, Bounds, Context, ElementId, Window,
    WindowBackgroundAppearance, WindowBounds, WindowKind, WindowOptions, div, ease_in_out, point,
    prelude::*, px, rgb, size,
};

const OVERLAY_WIDTH: f32 = 420.0;
const OVERLAY_HEIGHT: f32 = 88.0;
const BOTTOM_MARGIN: f32 = 56.0;

struct VoiceOverlay;

impl Render for VoiceOverlay {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        let waveform = div()
            .flex()
            .items_center()
            .justify_center()
            .gap_1()
            .w(px(92.0))
            .h(px(44.0))
            .children((0..11).map(|index| {
                let amplitude = 10.0 + ((index * 7) % 19) as f32;
                let duration = Duration::from_millis(540 + (index as u64 * 43));

                div()
                    .w(px(3.0))
                    .h(px(12.0))
                    .rounded_full()
                    .bg(rgb(0x2f6bff))
                    .with_animation(
                        ElementId::named_usize("wave", index),
                        Animation::new(duration).repeat().with_easing(ease_in_out),
                        move |bar, delta| {
                            let pulse = 1.0 - (2.0 * delta - 1.0).abs();
                            bar.h(px(8.0 + amplitude * pulse))
                        },
                    )
            }));

        div()
            .size_full()
            .flex()
            .items_center()
            .justify_center()
            .child(
                div()
                    .id("voice-capsule")
                    .flex()
                    .items_center()
                    .gap_4()
                    .w(px(396.0))
                    .h(px(72.0))
                    .px_5()
                    .rounded_full()
                    .bg(rgb(0xf8f9fc))
                    .border_1()
                    .border_color(rgb(0xdfe3ea))
                    .shadow_lg()
                    .text_color(rgb(0x20242c))
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .justify_center()
                            .size(px(40.0))
                            .rounded_full()
                            .bg(rgb(0xe9efff))
                            .child(
                                div()
                                    .w(px(12.0))
                                    .h(px(20.0))
                                    .rounded_full()
                                    .bg(rgb(0x2f6bff)),
                            ),
                    )
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .gap_1()
                            .w(px(154.0))
                            .child(div().text_base().child("Listening..."))
                            .child(
                                div()
                                    .text_sm()
                                    .text_color(rgb(0x747b87))
                                    .child("Ctrl+Alt+Space"),
                            ),
                    )
                    .child(waveform)
                    .child(
                        div()
                            .id("stop")
                            .flex()
                            .items_center()
                            .justify_center()
                            .size(px(40.0))
                            .rounded_full()
                            .bg(rgb(0x17191e))
                            .cursor_pointer()
                            .on_click(|_, window, _| platform::hide_overlay(window))
                            .child(div().size(px(12.0)).rounded_sm().bg(rgb(0xffffff))),
                    ),
            )
    }
}

fn overlay_bounds(cx: &App) -> Bounds<gpui::Pixels> {
    let overlay_size = size(px(OVERLAY_WIDTH), px(OVERLAY_HEIGHT));
    let Some(display) = cx.primary_display() else {
        return Bounds::centered(None, overlay_size, cx);
    };
    let display_bounds = display.bounds();
    Bounds {
        origin: point(
            display_bounds.origin.x + (display_bounds.size.width - overlay_size.width) / 2.0,
            display_bounds.origin.y + display_bounds.size.height
                - overlay_size.height
                - px(BOTTOM_MARGIN),
        ),
        size: overlay_size,
    }
}

fn main() {
    Application::new().run(|cx: &mut App| {
        let options = WindowOptions {
            window_bounds: Some(WindowBounds::Windowed(overlay_bounds(cx))),
            titlebar: None,
            focus: false,
            kind: WindowKind::PopUp,
            is_movable: false,
            is_resizable: false,
            is_minimizable: false,
            window_background: WindowBackgroundAppearance::Transparent,
            ..Default::default()
        };

        cx.open_window(options, |window, cx| {
            platform::configure_overlay(window);
            cx.new(|_| VoiceOverlay)
        })
        .expect("failed to open GPUI overlay window");
    });
}

#[cfg(target_os = "windows")]
mod platform {
    use std::ffi::c_void;
    use std::thread;

    use gpui::Window;
    use raw_window_handle::{HasWindowHandle, RawWindowHandle};
    use windows::Win32::Foundation::{HWND, LPARAM, WPARAM};
    use windows::Win32::UI::Input::KeyboardAndMouse::{
        MOD_ALT, MOD_CONTROL, RegisterHotKey, VK_SPACE,
    };
    use windows::Win32::UI::WindowsAndMessaging::{
        GWL_EXSTYLE, GetMessageW, GetWindowLongPtrW, HWND_TOPMOST, IsWindowVisible, MSG, SW_HIDE,
        SW_SHOWNOACTIVATE, SWP_FRAMECHANGED, SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOSIZE,
        SetWindowLongPtrW, SetWindowPos, ShowWindow, WM_HOTKEY, WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW,
    };

    const HOTKEY_ID: i32 = 0xDB01;

    pub fn configure_overlay(window: &Window) {
        let hwnd = hwnd(window);
        unsafe {
            let styles = GetWindowLongPtrW(hwnd, GWL_EXSTYLE);
            SetWindowLongPtrW(
                hwnd,
                GWL_EXSTYLE,
                styles | WS_EX_NOACTIVATE.0 as isize | WS_EX_TOOLWINDOW.0 as isize,
            );
            let _ = SetWindowPos(
                hwnd,
                Some(HWND_TOPMOST),
                0,
                0,
                0,
                0,
                SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE | SWP_FRAMECHANGED,
            );
        }
        start_hotkey_thread(hwnd.0 as isize);
    }

    pub fn hide_overlay(window: &Window) {
        unsafe {
            let _ = ShowWindow(hwnd(window), SW_HIDE);
        }
    }

    fn hwnd(window: &Window) -> HWND {
        match HasWindowHandle::window_handle(window)
            .expect("missing native window handle")
            .as_raw()
        {
            RawWindowHandle::Win32(handle) => HWND(handle.hwnd.get() as *mut c_void),
            _ => panic!("GPUI did not create a Win32 window"),
        }
    }

    fn start_hotkey_thread(hwnd_value: isize) {
        thread::spawn(move || unsafe {
            let hwnd = HWND(hwnd_value as *mut c_void);
            RegisterHotKey(None, HOTKEY_ID, MOD_CONTROL | MOD_ALT, VK_SPACE.0 as u32)
                .expect("failed to register Ctrl+Alt+Space");

            let mut message = MSG::default();
            while GetMessageW(&mut message, None, 0, 0).as_bool() {
                if message.message == WM_HOTKEY
                    && message.wParam == WPARAM(HOTKEY_ID as usize)
                    && message.lParam != LPARAM(0)
                {
                    if IsWindowVisible(hwnd).as_bool() {
                        let _ = ShowWindow(hwnd, SW_HIDE);
                    } else {
                        let _ = ShowWindow(hwnd, SW_SHOWNOACTIVATE);
                        let _ = SetWindowPos(
                            hwnd,
                            Some(HWND_TOPMOST),
                            0,
                            0,
                            0,
                            0,
                            SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE,
                        );
                    }
                }
            }
        });
    }
}

#[cfg(not(target_os = "windows"))]
mod platform {
    use gpui::Window;

    pub fn configure_overlay(_window: &Window) {}

    pub fn hide_overlay(window: &Window) {
        window.remove_window();
    }
}
