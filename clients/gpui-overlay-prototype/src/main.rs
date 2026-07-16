use std::time::Duration;

use gpui::{
    Animation, AnimationExt as _, App, Application, Bounds, Context, Window,
    WindowBackgroundAppearance, WindowBounds, WindowKind, WindowOptions, canvas, div, fill, point,
    prelude::*, px, rgb, size,
};

const OVERLAY_WIDTH: f32 = 432.0;
const OVERLAY_HEIGHT: f32 = 92.0;
const BOTTOM_MARGIN: f32 = 56.0;
const WAVEFORM_WIDTH: f32 = 88.0;
const WAVEFORM_HEIGHT: f32 = 40.0;
const BAR_WIDTH: f32 = 3.0;
const BAR_GAP: f32 = 4.0;
const BAR_COUNT: usize = 11;

fn waveform_canvas(delta: f32) -> impl IntoElement {
    const AMPLITUDES: [f32; BAR_COUNT] = [
        0.42, 0.62, 0.82, 0.68, 0.94, 0.76, 1.0, 0.72, 0.86, 0.58, 0.38,
    ];

    canvas(
        |_, _, _| {},
        move |bounds, _, window, _| {
            let phase = delta * std::f32::consts::TAU;
            let bars_width = BAR_COUNT as f32 * BAR_WIDTH + (BAR_COUNT - 1) as f32 * BAR_GAP;
            let start_x = bounds.origin.x + (bounds.size.width - px(bars_width)) / 2.0;
            let center_y = bounds.origin.y + bounds.size.height / 2.0;

            for (index, amplitude) in AMPLITUDES.iter().enumerate() {
                let offset = index as f32 * 0.68;
                let primary = ((phase + offset).sin() + 1.0) * 0.5;
                let secondary = ((phase * 2.0 - offset * 0.8).sin() + 1.0) * 0.5;
                let energy = 0.64 * primary + 0.36 * secondary;
                let height = 7.0 + 25.0 * amplitude * (0.28 + 0.72 * energy);
                let bar_bounds = Bounds::new(
                    point(
                        start_x + px(index as f32 * (BAR_WIDTH + BAR_GAP)),
                        center_y - px(height / 2.0),
                    ),
                    size(px(BAR_WIDTH), px(height)),
                );

                window
                    .paint_quad(fill(bar_bounds, rgb(0x2f6bff)).corner_radii(px(BAR_WIDTH / 2.0)));
            }
        },
    )
    .w(px(WAVEFORM_WIDTH))
    .h(px(WAVEFORM_HEIGHT))
}

struct VoiceOverlay;

impl Render for VoiceOverlay {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        let waveform = div()
            .w(px(WAVEFORM_WIDTH))
            .h(px(WAVEFORM_HEIGHT))
            .with_animation(
                "waveform",
                Animation::new(Duration::from_millis(1_180)).repeat(),
                |surface, delta| surface.child(waveform_canvas(delta)),
            );

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
                    .justify_between()
                    .w(px(404.0))
                    .h(px(74.0))
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
                            .w(px(132.0))
                            .child(div().text_base().child("Listening"))
                            .child(
                                div()
                                    .text_sm()
                                    .text_color(rgb(0x747b87))
                                    .child("Remote microphone"),
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
