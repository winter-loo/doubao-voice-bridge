#![cfg_attr(
    all(target_os = "windows", not(debug_assertions)),
    windows_subsystem = "windows"
)]

use std::{
    sync::atomic::{AtomicU8, AtomicU64, Ordering},
    time::Duration,
};

use gpui::{
    Animation, AnimationExt as _, App, Application, Bounds, Context, Window,
    WindowBackgroundAppearance, WindowBounds, WindowKind, WindowOptions, canvas, div, fill, point,
    prelude::*, px, rgb, rgba, size,
};

const OVERLAY_WIDTH: f32 = 266.0;
const OVERLAY_HEIGHT: f32 = 48.0;
const BOTTOM_MARGIN: f32 = 30.0;
const WAVEFORM_HEIGHT: f32 = 22.0;
const BAR_WIDTH: f32 = 2.0;
const BAR_GAP: f32 = 2.0;
const BAR_COUNT: usize = 20;
const WAVEFORM_BARS_WIDTH: f32 = BAR_COUNT as f32 * BAR_WIDTH + (BAR_COUNT - 1) as f32 * BAR_GAP;
const LISTENING_HORIZONTAL_PADDING: f32 = 24.0;
const LISTENING_CAPSULE_WIDTH: f32 = WAVEFORM_BARS_WIDTH + LISTENING_HORIZONTAL_PADDING;

#[derive(Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
enum OverlayPhase {
    Hidden,
    Listening,
    Optimizing,
}

static OVERLAY_PHASE: AtomicU8 = AtomicU8::new(OverlayPhase::Hidden as u8);
static OVERLAY_GENERATION: AtomicU64 = AtomicU64::new(0);

fn overlay_phase() -> OverlayPhase {
    match OVERLAY_PHASE.load(Ordering::Acquire) {
        1 => OverlayPhase::Listening,
        2 => OverlayPhase::Optimizing,
        _ => OverlayPhase::Hidden,
    }
}

fn set_overlay_phase(phase: OverlayPhase) {
    OVERLAY_PHASE.store(phase as u8, Ordering::Release);
}

fn next_overlay_generation() -> u64 {
    OVERLAY_GENERATION.fetch_add(1, Ordering::AcqRel) + 1
}

fn overlay_generation() -> u64 {
    OVERLAY_GENERATION.load(Ordering::Acquire)
}

fn lerp_rgb(start: u32, end: u32, t: f32) -> u32 {
    let channel = |shift: u32| {
        let from = ((start >> shift) & 0xffu32) as f32;
        let to = ((end >> shift) & 0xffu32) as f32;
        (from + (to - from) * t).round() as u32
    };
    (channel(16) << 16) | (channel(8) << 8) | channel(0)
}

fn waveform_canvas(delta: f32) -> impl IntoElement {
    const AMPLITUDES: [f32; BAR_COUNT] = [
        0.24, 0.32, 0.44, 0.58, 0.42, 0.64, 0.88, 1.0, 0.78, 0.56, 0.92, 0.74, 0.58, 0.68, 0.49,
        0.42, 0.35, 0.3, 0.25, 0.2,
    ];

    canvas(
        |_, _, _| {},
        move |bounds, _, window, _| {
            let phase = delta * std::f32::consts::TAU;
            let start_x = bounds.origin.x + (bounds.size.width - px(WAVEFORM_BARS_WIDTH)) / 2.0;
            let center_y = bounds.origin.y + bounds.size.height / 2.0;

            for (index, amplitude) in AMPLITUDES.iter().enumerate() {
                let offset = index as f32 * 0.53;
                let primary = ((phase + offset).sin() + 1.0) * 0.5;
                let secondary = ((phase * 2.0 - offset * 0.8).sin() + 1.0) * 0.5;
                let energy = 0.7 * primary + 0.3 * secondary;
                let height = 4.0 + 18.0 * amplitude * (0.12 + 0.88 * energy);
                let bar_bounds = Bounds::new(
                    point(
                        start_x + px(index as f32 * (BAR_WIDTH + BAR_GAP)),
                        center_y - px(height / 2.0),
                    ),
                    size(px(BAR_WIDTH), px(height)),
                );

                let color = lerp_rgb(0x43ded2, 0x648dff, index as f32 / (BAR_COUNT - 1) as f32);
                window.paint_quad(fill(bar_bounds, rgb(color)).corner_radii(px(BAR_WIDTH / 2.0)));
            }
        },
    )
    .w(px(LISTENING_CAPSULE_WIDTH))
    .h(px(WAVEFORM_HEIGHT))
}

fn capsule_base() -> gpui::Div {
    div()
        .flex()
        .items_center()
        .justify_center()
        .rounded_full()
        .bg(rgba(0x111318f2))
        .border_1()
        .border_color(rgba(0xffffff24))
        .shadow_lg()
        .text_color(rgb(0xf7f8fb))
}

fn listening_capsule(delta: f32) -> impl IntoElement {
    capsule_base()
        .id("voice-capsule")
        .w(px(LISTENING_CAPSULE_WIDTH))
        .h(px(34.0))
        .cursor_pointer()
        .on_click(|_, window, _| platform::finish_input(window))
        .child(waveform_canvas(delta))
}

fn optimizing_capsule() -> impl IntoElement {
    capsule_base()
        .id("voice-capsule")
        .w(px(108.0))
        .h(px(30.0))
        .text_sm()
        .child("优化识别中")
}

struct VoiceOverlay;

impl Render for VoiceOverlay {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .size_full()
            .flex()
            .items_center()
            .justify_center()
            .with_animation(
                "overlay-clock",
                Animation::new(Duration::from_millis(1_120)).repeat(),
                |root, delta| {
                    let content = match overlay_phase() {
                        OverlayPhase::Hidden => div().into_any_element(),
                        OverlayPhase::Listening => listening_capsule(delta).into_any_element(),
                        OverlayPhase::Optimizing => optimizing_capsule().into_any_element(),
                    };
                    root.child(content)
                },
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
    use std::time::{Duration, Instant};

    use super::{
        OverlayPhase, next_overlay_generation, overlay_generation, overlay_phase, set_overlay_phase,
    };
    use gpui::Window;
    use raw_window_handle::{HasWindowHandle, RawWindowHandle};
    use windows::Win32::Foundation::{HWND, LPARAM, WPARAM};
    use windows::Win32::UI::Input::KeyboardAndMouse::{
        GetAsyncKeyState, MOD_ALT, MOD_CONTROL, RegisterHotKey, VK_RCONTROL, VK_SPACE,
    };
    use windows::Win32::UI::WindowsAndMessaging::{
        GWL_EXSTYLE, GWL_STYLE, GetMessageW, GetWindowLongPtrW, HWND_TOPMOST, IsWindowVisible, MSG,
        SW_HIDE, SW_SHOWNOACTIVATE, SWP_FRAMECHANGED, SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOSIZE,
        SetWindowLongPtrW, SetWindowPos, ShowWindow, WM_HOTKEY, WS_BORDER, WS_DLGFRAME,
        WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW, WS_THICKFRAME,
    };

    const HOTKEY_ID: i32 = 0xDB01;
    const HOLD_THRESHOLD: Duration = Duration::from_millis(420);
    const OPTIMIZING_DURATION: Duration = Duration::from_millis(2_400);

    pub fn configure_overlay(window: &Window) {
        let hwnd = hwnd(window);
        unsafe {
            let styles = GetWindowLongPtrW(hwnd, GWL_STYLE);
            let frame_styles =
                WS_BORDER.0 as isize | WS_DLGFRAME.0 as isize | WS_THICKFRAME.0 as isize;
            SetWindowLongPtrW(hwnd, GWL_STYLE, styles & !frame_styles);

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
        hide_overlay(hwnd);
        start_hotkey_thread(hwnd.0 as isize);
        start_hold_key_thread(hwnd.0 as isize);
    }

    pub fn finish_input(window: &Window) {
        finish_input_hwnd(hwnd(window));
    }

    fn show_phase(hwnd: HWND, phase: OverlayPhase) {
        set_overlay_phase(phase);
        unsafe {
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

    fn hide_overlay(hwnd: HWND) {
        set_overlay_phase(OverlayPhase::Hidden);
        unsafe {
            let _ = ShowWindow(hwnd, SW_HIDE);
        }
    }

    fn begin_input(hwnd: HWND) {
        next_overlay_generation();
        show_phase(hwnd, OverlayPhase::Listening);
    }

    fn finish_input_hwnd(hwnd: HWND) {
        match overlay_phase() {
            OverlayPhase::Hidden => return,
            OverlayPhase::Optimizing => {
                hide_overlay(hwnd);
                return;
            }
            OverlayPhase::Listening => {}
        }

        let generation = overlay_generation();
        show_phase(hwnd, OverlayPhase::Optimizing);
        let hwnd_value = hwnd.0 as isize;
        thread::spawn(move || {
            thread::sleep(OPTIMIZING_DURATION);
            if overlay_generation() == generation && overlay_phase() == OverlayPhase::Optimizing {
                hide_overlay(HWND(hwnd_value as *mut c_void));
            }
        });
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
                        finish_input_hwnd(hwnd);
                    } else {
                        begin_input(hwnd);
                    }
                }
            }
        });
    }

    fn start_hold_key_thread(hwnd_value: isize) {
        thread::spawn(move || {
            let hwnd = HWND(hwnd_value as *mut c_void);
            let mut pressed_at = None;
            let mut activated = false;

            loop {
                let is_down = unsafe { GetAsyncKeyState(VK_RCONTROL.0 as i32) < 0 };

                if is_down {
                    let started = pressed_at.get_or_insert_with(Instant::now);
                    if !activated && started.elapsed() >= HOLD_THRESHOLD {
                        activated = true;
                        begin_input(hwnd);
                    }
                } else if pressed_at.take().is_some() {
                    if activated {
                        finish_input_hwnd(hwnd);
                    }
                    activated = false;
                }

                thread::sleep(Duration::from_millis(16));
            }
        });
    }
}

#[cfg(not(target_os = "windows"))]
mod platform {
    use gpui::Window;

    use super::{OverlayPhase, set_overlay_phase};

    pub fn configure_overlay(_window: &Window) {
        set_overlay_phase(OverlayPhase::Listening);
    }

    pub fn finish_input(window: &Window) {
        window.remove_window();
    }
}
