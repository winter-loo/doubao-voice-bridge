#![cfg_attr(
    all(target_os = "windows", not(debug_assertions)),
    windows_subsystem = "windows"
)]

use std::{
    sync::{
        Arc, OnceLock, RwLock,
        atomic::{AtomicBool, AtomicU8, AtomicU64, Ordering},
    },
    time::Duration,
};

use gpui::{
    Animation, AnimationExt as _, App, Application, Bounds, ColorSpace, Context, FontWeight,
    RenderImage, Window, WindowBackgroundAppearance, WindowBounds, WindowKind, WindowOptions,
    canvas, div, fill, img, linear_color_stop, linear_gradient, point, prelude::*, px, rgb, rgba,
    size,
};

const BOTTOM_MARGIN: f32 = 22.0;
const BAR_WIDTH: f32 = 2.0;
const BAR_GAP: f32 = 2.0;
const BAR_COUNT: usize = 20;
const WAVEFORM_BARS_WIDTH: f32 = BAR_COUNT as f32 * BAR_WIDTH + (BAR_COUNT - 1) as f32 * BAR_GAP;
const LISTENING_HORIZONTAL_PADDING: f32 = 30.0;
const LISTENING_CAPSULE_WIDTH: f32 = WAVEFORM_BARS_WIDTH + LISTENING_HORIZONTAL_PADDING;
const LISTENING_CAPSULE_HEIGHT: f32 = 26.0;
const OVERLAY_WIDTH: f32 = 128.0;
const OVERLAY_HEIGHT: f32 = 42.0;

#[derive(Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
enum OverlayPhase {
    Hidden,
    Listening,
    Optimizing,
}

static OVERLAY_PHASE: AtomicU8 = AtomicU8::new(OverlayPhase::Hidden as u8);
static OVERLAY_GENERATION: AtomicU64 = AtomicU64::new(0);
static GLASS_BACKDROP: OnceLock<RwLock<Option<Arc<RenderImage>>>> = OnceLock::new();
static GLASS_BACKDROP_IS_DARK: AtomicBool = AtomicBool::new(false);

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

fn glass_backdrop() -> Option<Arc<RenderImage>> {
    GLASS_BACKDROP
        .get_or_init(|| RwLock::new(None))
        .read()
        .expect("glass backdrop lock poisoned")
        .clone()
}

fn set_glass_backdrop(backdrop: Option<Arc<RenderImage>>) {
    *GLASS_BACKDROP
        .get_or_init(|| RwLock::new(None))
        .write()
        .expect("glass backdrop lock poisoned") = backdrop;
}

fn glass_backdrop_is_dark() -> bool {
    GLASS_BACKDROP_IS_DARK.load(Ordering::Acquire)
}

fn set_glass_backdrop_is_dark(is_dark: bool) {
    GLASS_BACKDROP_IS_DARK.store(is_dark, Ordering::Release);
}

fn lerp_rgb(start: u32, end: u32, t: f32) -> u32 {
    let channel = |shift: u32| {
        let from = ((start >> shift) & 0xffu32) as f32;
        let to = ((end >> shift) & 0xffu32) as f32;
        (from + (to - from) * t).round() as u32
    };
    (channel(16) << 16) | (channel(8) << 8) | channel(0)
}

fn glass_canvas(delta: f32, show_waveform: bool) -> impl IntoElement + Styled {
    const AMPLITUDES: [f32; BAR_COUNT] = [
        0.24, 0.32, 0.44, 0.58, 0.42, 0.64, 0.88, 1.0, 0.78, 0.56, 0.92, 0.74, 0.58, 0.68, 0.49,
        0.42, 0.35, 0.3, 0.25, 0.2,
    ];

    canvas(
        |_, _, _| {},
        move |bounds, _, window, _| {
            let phase = delta * std::f32::consts::TAU;
            let radius = bounds.size.height / 2.0;
            window.paint_quad(
                fill(
                    bounds,
                    linear_gradient(
                        180.0,
                        linear_color_stop(rgba(0xffffff47), 0.0),
                        linear_color_stop(rgba(0xc8ebff24), 1.0),
                    )
                    .color_space(ColorSpace::Oklab),
                )
                .corner_radii(radius),
            );

            let top_lens = Bounds::new(
                bounds.origin + point(px(2.0), px(1.0)),
                size(bounds.size.width - px(4.0), px(9.0)),
            );
            window.paint_quad(
                fill(
                    top_lens,
                    linear_gradient(
                        180.0,
                        linear_color_stop(rgba(0xffffff8f), 0.0),
                        linear_color_stop(rgba(0xffffff00), 1.0),
                    )
                    .color_space(ColorSpace::Oklab),
                )
                .corner_radii(radius),
            );

            let lower_reflection = Bounds::new(
                bounds.origin + point(px(3.0), bounds.size.height - px(8.0)),
                size(bounds.size.width - px(6.0), px(6.0)),
            );
            window.paint_quad(
                fill(
                    lower_reflection,
                    linear_gradient(
                        180.0,
                        linear_color_stop(rgba(0xffffff00), 0.0),
                        linear_color_stop(rgba(0xbdeeff36), 1.0),
                    )
                    .color_space(ColorSpace::Oklab),
                )
                .corner_radii(radius),
            );

            let sheen_progress = 0.5 + 0.5 * (phase * 0.42).sin();
            let sheen_x = bounds.origin.x + px(12.0 + 78.0 * sheen_progress);
            for (offset, alpha) in [(-3.0, 0x0a), (0.0, 0x32), (3.0, 0x0d)] {
                let sheen = Bounds::new(
                    point(sheen_x + px(offset), bounds.origin.y + px(3.0)),
                    size(px(2.0), bounds.size.height - px(6.0)),
                );
                window.paint_quad(
                    fill(
                        sheen,
                        linear_gradient(
                            160.0,
                            linear_color_stop(rgba(0xffffff00 | alpha), 0.0),
                            linear_color_stop(rgba(0xbdeeff00 | alpha), 1.0),
                        ),
                    )
                    .corner_radii(px(1.0)),
                );
            }

            let upper_rim = Bounds::new(
                bounds.origin + point(px(13.0), px(1.0)),
                size(bounds.size.width - px(26.0), px(1.0)),
            );
            window.paint_quad(
                fill(
                    upper_rim,
                    linear_gradient(
                        90.0,
                        linear_color_stop(rgba(0xffffff38), 0.0),
                        linear_color_stop(rgba(0xffffffd6), 0.5),
                    ),
                )
                .corner_radii(px(0.5)),
            );

            let lower_rim = Bounds::new(
                bounds.origin + point(px(15.0), bounds.size.height - px(2.0)),
                size(bounds.size.width - px(30.0), px(1.0)),
            );
            window.paint_quad(
                fill(
                    lower_rim,
                    linear_gradient(
                        90.0,
                        linear_color_stop(rgba(0xffffff24), 0.0),
                        linear_color_stop(rgba(0x9fdaff70), 1.0),
                    ),
                )
                .corner_radii(px(0.5)),
            );

            for x in [
                bounds.origin.x + px(1.0),
                bounds.origin.x + bounds.size.width - px(2.0),
            ] {
                let edge_caustic = Bounds::new(
                    point(x, bounds.origin.y + px(6.0)),
                    size(px(1.0), bounds.size.height - px(12.0)),
                );
                window.paint_quad(fill(edge_caustic, rgba(0xffffff8a)).corner_radii(px(0.5)));
            }

            if !show_waveform {
                return;
            }

            let start_x = bounds.origin.x + (bounds.size.width - px(WAVEFORM_BARS_WIDTH)) / 2.0;
            let center_y = bounds.origin.y + bounds.size.height / 2.0;

            for (index, amplitude) in AMPLITUDES.iter().enumerate() {
                let offset = index as f32 * 0.53;
                let primary = ((phase + offset).sin() + 1.0) * 0.5;
                let secondary = ((phase * 2.0 - offset * 0.8).sin() + 1.0) * 0.5;
                let energy = 0.7 * primary + 0.3 * secondary;
                let height = 3.0 + 13.0 * amplitude * (0.12 + 0.88 * energy);
                let bar_bounds = Bounds::new(
                    point(
                        start_x + px(index as f32 * (BAR_WIDTH + BAR_GAP)),
                        center_y - px(height / 2.0),
                    ),
                    size(px(BAR_WIDTH), px(height)),
                );

                let color = lerp_rgb(0x43ded2, 0x648dff, index as f32 / (BAR_COUNT - 1) as f32);
                let glow_bounds = Bounds::new(
                    bar_bounds.origin - point(px(1.0), px(1.0)),
                    bar_bounds.size + size(px(2.0), px(2.0)),
                );
                window.paint_quad(
                    fill(glow_bounds, rgba((color << 8) | 0x2e)).corner_radii(px(BAR_WIDTH)),
                );
                window.paint_quad(fill(bar_bounds, rgb(color)).corner_radii(px(BAR_WIDTH / 2.0)));
            }
        },
    )
    .w(px(LISTENING_CAPSULE_WIDTH))
    .h(px(LISTENING_CAPSULE_HEIGHT))
}

fn capsule_base() -> gpui::Div {
    let border_color = if glass_backdrop_is_dark() {
        rgba(0xffffffb8)
    } else {
        rgba(0x526a806b)
    };

    div()
        .relative()
        .flex()
        .items_center()
        .justify_center()
        .rounded_full()
        .overflow_hidden()
        .bg(linear_gradient(
            180.0,
            linear_color_stop(rgba(0xffffff52), 0.0),
            linear_color_stop(rgba(0xffffff3d), 1.0),
        )
        .color_space(ColorSpace::Oklab))
        .border_1()
        .border_color(border_color)
        .shadow_lg()
        .text_color(rgba(0x07131ff5))
}

fn backdrop_element() -> gpui::AnyElement {
    match glass_backdrop() {
        Some(backdrop) => img(backdrop)
            .absolute()
            .top_0()
            .left_0()
            .size_full()
            .into_any_element(),
        None => div()
            .absolute()
            .top_0()
            .left_0()
            .size_full()
            .bg(rgba(0xffffff2e))
            .into_any_element(),
    }
}

fn listening_capsule(delta: f32) -> impl IntoElement {
    capsule_base()
        .id("voice-capsule")
        .w(px(LISTENING_CAPSULE_WIDTH))
        .h(px(LISTENING_CAPSULE_HEIGHT))
        .cursor_pointer()
        .on_click(|_, window, _| platform::finish_input(window))
        .child(backdrop_element())
        .child(glass_canvas(delta, true).relative())
}

fn optimizing_capsule(delta: f32) -> impl IntoElement {
    capsule_base()
        .id("voice-capsule")
        .relative()
        .w(px(LISTENING_CAPSULE_WIDTH))
        .h(px(LISTENING_CAPSULE_HEIGHT))
        .child(backdrop_element())
        .child(glass_canvas(delta, false).absolute().top_0().left_0())
        .child(
            div()
                .relative()
                .text_xs()
                .font_weight(FontWeight::MEDIUM)
                .child("优化识别中"),
        )
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
                        OverlayPhase::Optimizing => optimizing_capsule(delta).into_any_element(),
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
    use std::sync::Arc;
    use std::thread;
    use std::time::{Duration, Instant};

    use super::{
        LISTENING_CAPSULE_HEIGHT, LISTENING_CAPSULE_WIDTH, OVERLAY_HEIGHT, OVERLAY_WIDTH,
        OverlayPhase, next_overlay_generation, overlay_generation, overlay_phase,
        set_glass_backdrop, set_glass_backdrop_is_dark, set_overlay_phase,
    };
    use gpui::{RenderImage, Window};
    use image::{Frame, RgbaImage, imageops};
    use raw_window_handle::{HasWindowHandle, RawWindowHandle};
    use windows::Win32::Foundation::{HWND, LPARAM, WPARAM};
    use windows::Win32::Graphics::Gdi::{
        BI_RGB, BITMAPINFO, BITMAPINFOHEADER, BitBlt, CreateCompatibleBitmap, CreateCompatibleDC,
        DIB_RGB_COLORS, DeleteDC, DeleteObject, GetDC, GetDIBits, GetMonitorInfoW, HGDIOBJ,
        MONITOR_DEFAULTTOPRIMARY, MONITORINFO, MonitorFromWindow, ReleaseDC, SRCCOPY, SelectObject,
    };
    use windows::Win32::UI::Input::KeyboardAndMouse::{
        GetAsyncKeyState, MOD_ALT, MOD_CONTROL, RegisterHotKey, VK_RCONTROL, VK_SPACE,
    };
    use windows::Win32::UI::WindowsAndMessaging::{
        GWL_EXSTYLE, GWL_STYLE, GetMessageW, GetWindowLongPtrW, GetWindowRect, HWND_TOPMOST,
        IsWindowVisible, MSG, SW_HIDE, SW_SHOWNOACTIVATE, SWP_FRAMECHANGED, SWP_NOACTIVATE,
        SWP_NOMOVE, SWP_NOSIZE, SetWindowLongPtrW, SetWindowPos, ShowWindow, WM_HOTKEY, WS_BORDER,
        WS_DLGFRAME, WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW, WS_THICKFRAME,
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
            position_above_work_area(hwnd);
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
        capture_glass_backdrop(hwnd);
        start_hotkey_thread(hwnd.0 as isize);
        start_hold_key_thread(hwnd.0 as isize);
    }

    fn position_above_work_area(hwnd: HWND) {
        unsafe {
            let monitor = MonitorFromWindow(hwnd, MONITOR_DEFAULTTOPRIMARY);
            let mut monitor_info = MONITORINFO {
                cbSize: std::mem::size_of::<MONITORINFO>() as u32,
                ..Default::default()
            };
            let mut window_rect = Default::default();
            if !GetMonitorInfoW(monitor, &mut monitor_info).as_bool()
                || GetWindowRect(hwnd, &mut window_rect).is_err()
            {
                return;
            }

            let window_width = window_rect.right - window_rect.left;
            let window_height = window_rect.bottom - window_rect.top;
            let scale = window_width as f32 / OVERLAY_WIDTH;
            let work_area = monitor_info.rcWork;
            let x = work_area.left + (work_area.right - work_area.left - window_width) / 2;
            let y = work_area.bottom - window_height - (8.0 * scale).round() as i32;
            let _ = SetWindowPos(hwnd, None, x, y, 0, 0, SWP_NOSIZE | SWP_NOACTIVATE);
        }
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
        capture_glass_backdrop(hwnd);
        show_phase(hwnd, OverlayPhase::Listening);
    }

    fn capture_glass_backdrop(hwnd: HWND) {
        let mut window_rect = Default::default();
        if unsafe { GetWindowRect(hwnd, &mut window_rect) }.is_err() {
            set_glass_backdrop_is_dark(false);
            set_glass_backdrop(None);
            return;
        }

        let window_width = window_rect.right - window_rect.left;
        let window_height = window_rect.bottom - window_rect.top;
        if window_width <= 0 || window_height <= 0 {
            set_glass_backdrop_is_dark(false);
            set_glass_backdrop(None);
            return;
        }

        let scale_x = window_width as f32 / OVERLAY_WIDTH;
        let scale_y = window_height as f32 / OVERLAY_HEIGHT;
        let capsule_width = (LISTENING_CAPSULE_WIDTH * scale_x).round() as i32;
        let capsule_height = (LISTENING_CAPSULE_HEIGHT * scale_y).round() as i32;

        // Sample slightly inside the capsule footprint, then stretch it back out.
        // The small magnification is the refraction cue; blur alone reads as Acrylic.
        let source_width = (capsule_width - (6.0 * scale_x).round() as i32).max(1);
        let source_height = (capsule_height - (4.0 * scale_y).round() as i32).max(1);
        let source_x = window_rect.left + (window_width - source_width) / 2;
        let source_y = window_rect.top + (window_height - source_height) / 2;

        let backdrop = capture_screen_region(source_x, source_y, source_width, source_height);
        if backdrop.is_none() {
            set_glass_backdrop_is_dark(false);
        }
        set_glass_backdrop(backdrop);
    }

    fn capture_screen_region(x: i32, y: i32, width: i32, height: i32) -> Option<Arc<RenderImage>> {
        unsafe {
            let screen_dc = GetDC(None);
            if screen_dc.is_invalid() {
                return None;
            }

            let memory_dc = CreateCompatibleDC(Some(screen_dc));
            if memory_dc.is_invalid() {
                let _ = ReleaseDC(None, screen_dc);
                return None;
            }

            let bitmap = CreateCompatibleBitmap(screen_dc, width, height);
            if bitmap.is_invalid() {
                let _ = DeleteDC(memory_dc);
                let _ = ReleaseDC(None, screen_dc);
                return None;
            }

            let previous = SelectObject(memory_dc, HGDIOBJ(bitmap.0));
            let copied = BitBlt(
                memory_dc,
                0,
                0,
                width,
                height,
                Some(screen_dc),
                x,
                y,
                SRCCOPY,
            )
            .is_ok();

            let mut bitmap_info = BITMAPINFO {
                bmiHeader: BITMAPINFOHEADER {
                    biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
                    biWidth: width,
                    biHeight: -height,
                    biPlanes: 1,
                    biBitCount: 32,
                    biCompression: BI_RGB.0,
                    ..Default::default()
                },
                ..Default::default()
            };
            let mut pixels = vec![0u8; (width * height * 4) as usize];
            let scan_lines = if copied {
                GetDIBits(
                    memory_dc,
                    bitmap,
                    0,
                    height as u32,
                    Some(pixels.as_mut_ptr().cast()),
                    &mut bitmap_info,
                    DIB_RGB_COLORS,
                )
            } else {
                0
            };

            let _ = SelectObject(memory_dc, previous);
            let _ = DeleteObject(bitmap.into());
            let _ = DeleteDC(memory_dc);
            let _ = ReleaseDC(None, screen_dc);

            if scan_lines == 0 {
                return None;
            }

            let mut luma_sum = 0u64;
            for pixel in pixels.chunks_exact_mut(4) {
                pixel.swap(0, 2);
                luma_sum +=
                    (pixel[0] as u64 * 54 + pixel[1] as u64 * 183 + pixel[2] as u64 * 19) >> 8;
                pixel[0] = ((pixel[0] as u16 * 91 + 255 * 9) / 100) as u8;
                pixel[1] = ((pixel[1] as u16 * 91 + 255 * 9) / 100) as u8;
                pixel[2] = ((pixel[2] as u16 * 91 + 255 * 9) / 100) as u8;
                pixel[3] = 255;
            }
            let pixel_count = (width as u64 * height as u64).max(1);
            set_glass_backdrop_is_dark(luma_sum / pixel_count < 144);

            let image = RgbaImage::from_raw(width as u32, height as u32, pixels)?;
            let blurred = imageops::blur(&image, 4.5);
            Some(Arc::new(RenderImage::new(vec![Frame::new(blurred)])))
        }
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
