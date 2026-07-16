#![cfg_attr(
    all(target_os = "windows", not(debug_assertions)),
    windows_subsystem = "windows"
)]

use std::{
    sync::atomic::{AtomicBool, AtomicU8, AtomicU64, Ordering},
    time::Duration,
};

use gpui::{
    Animation, AnimationExt as _, App, Application, Bounds, ColorSpace, Context, FontWeight,
    Window, WindowBackgroundAppearance, WindowBounds, WindowKind, WindowOptions, canvas, div, fill,
    linear_color_stop, linear_gradient, point, prelude::*, px, rgb, rgba, size,
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
static DARK_BACKGROUND: AtomicBool = AtomicBool::new(false);

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

fn dark_background() -> bool {
    DARK_BACKGROUND.load(Ordering::Acquire)
}

fn set_dark_background(dark: bool) {
    DARK_BACKGROUND.store(dark, Ordering::Release);
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
            let dark = dark_background();
            let (glass_top, glass_bottom) = if dark {
                (rgba(0x5f7f966e), rgba(0x07121f9c))
            } else {
                (rgba(0xffffff38), rgba(0xd8efff1c))
            };
            window.paint_quad(
                fill(
                    bounds,
                    linear_gradient(
                        180.0,
                        linear_color_stop(glass_top, 0.0),
                        linear_color_stop(glass_bottom, 1.0),
                    )
                    .color_space(ColorSpace::Oklab),
                )
                .corner_radii(radius),
            );

            let top_lens = Bounds::new(
                bounds.origin + point(px(4.0), px(1.0)),
                size(bounds.size.width - px(8.0), px(10.0)),
            );
            window.paint_quad(
                fill(
                    top_lens,
                    linear_gradient(
                        180.0,
                        linear_color_stop(
                            if dark {
                                rgba(0xffffffec)
                            } else {
                                rgba(0xffffffb8)
                            },
                            0.0,
                        ),
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
                        linear_color_stop(
                            if dark {
                                rgba(0xa7dcff68)
                            } else {
                                rgba(0xffffff4c)
                            },
                            1.0,
                        ),
                    )
                    .color_space(ColorSpace::Oklab),
                )
                .corner_radii(radius),
            );

            let left_refraction = Bounds::new(
                bounds.origin + point(px(1.0), px(4.0)),
                size(px(8.0), bounds.size.height - px(8.0)),
            );
            window.paint_quad(
                fill(
                    left_refraction,
                    linear_gradient(
                        90.0,
                        linear_color_stop(rgba(0x8ee9ff42), 0.0),
                        linear_color_stop(rgba(0xffffff00), 1.0),
                    )
                    .color_space(ColorSpace::Oklab),
                )
                .corner_radii(radius),
            );

            let right_refraction = Bounds::new(
                point(
                    bounds.origin.x + bounds.size.width - px(9.0),
                    bounds.origin.y + px(4.0),
                ),
                size(px(8.0), bounds.size.height - px(8.0)),
            );
            window.paint_quad(
                fill(
                    right_refraction,
                    linear_gradient(
                        270.0,
                        linear_color_stop(rgba(0xffc8f12e), 0.0),
                        linear_color_stop(rgba(0xffffff00), 1.0),
                    )
                    .color_space(ColorSpace::Oklab),
                )
                .corner_radii(radius),
            );

            let upper_rim = Bounds::new(
                bounds.origin + point(px(13.0), px(1.0)),
                size(bounds.size.width - px(26.0), px(1.0)),
            );
            window.paint_quad(
                fill(
                    upper_rim,
                    linear_gradient(
                        90.0,
                        linear_color_stop(rgba(0xffffff22), 0.0),
                        linear_color_stop(rgba(0xffffffea), 0.5),
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
                        linear_color_stop(rgba(0xaeeaff18), 0.0),
                        linear_color_stop(rgba(0xffffff78), 0.55),
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
                window.paint_quad(fill(edge_caustic, rgba(0xffffff62)).corner_radii(px(0.5)));
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
                    fill(glow_bounds, rgba((color << 8) | 0x24)).corner_radii(px(BAR_WIDTH)),
                );
                window.paint_quad(fill(bar_bounds, rgb(color)).corner_radii(px(BAR_WIDTH / 2.0)));
            }
        },
    )
    .w(px(LISTENING_CAPSULE_WIDTH))
    .h(px(LISTENING_CAPSULE_HEIGHT))
}

fn capsule_base() -> gpui::Div {
    let dark = dark_background();
    div()
        .relative()
        .flex()
        .items_center()
        .justify_center()
        .rounded_full()
        .overflow_hidden()
        .bg(if dark {
            rgba(0x06101b38)
        } else {
            rgba(0xffffff0c)
        })
        .border_1()
        .border_color(if dark {
            rgba(0xffffffe0)
        } else {
            rgba(0xaec8d55a)
        })
        .text_color(if dark {
            rgba(0xffffffff)
        } else {
            rgba(0x07131ff5)
        })
}

fn listening_capsule(delta: f32) -> impl IntoElement {
    capsule_base()
        .id("voice-capsule")
        .w(px(LISTENING_CAPSULE_WIDTH))
        .h(px(LISTENING_CAPSULE_HEIGHT))
        .cursor_pointer()
        .on_click(|_, window, _| platform::finish_input(window))
        .child(glass_canvas(delta, true).relative())
}

fn optimizing_capsule(delta: f32) -> impl IntoElement {
    capsule_base()
        .id("voice-capsule")
        .relative()
        .w(px(LISTENING_CAPSULE_WIDTH))
        .h(px(LISTENING_CAPSULE_HEIGHT))
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
    use std::fs::OpenOptions;
    use std::io::Write as _;
    use std::os::windows::process::CommandExt as _;
    use std::path::PathBuf;
    use std::process::{Child, ChildStdin, Command, Stdio};
    use std::sync::Mutex;
    use std::sync::atomic::{AtomicIsize, Ordering};
    use std::thread;
    use std::time::{Duration, Instant};

    use super::{
        LISTENING_CAPSULE_HEIGHT, LISTENING_CAPSULE_WIDTH, OVERLAY_HEIGHT, OVERLAY_WIDTH,
        OverlayPhase, dark_background, next_overlay_generation, overlay_generation, overlay_phase,
        set_dark_background, set_overlay_phase,
    };
    use gpui::Window;
    use raw_window_handle::{HasWindowHandle, RawWindowHandle};
    use windows::Win32::Foundation::{HWND, LPARAM, RECT, WPARAM};
    use windows::Win32::Graphics::Dwm::{
        DWM_THUMBNAIL_PROPERTIES, DWM_TNP_OPACITY, DWM_TNP_RECTDESTINATION, DWM_TNP_RECTSOURCE,
        DWM_TNP_SOURCECLIENTAREAONLY, DWM_TNP_VISIBLE, DWMNCRP_DISABLED, DWMWA_BORDER_COLOR,
        DWMWA_COLOR_NONE, DWMWA_NCRENDERING_POLICY, DwmRegisterThumbnail, DwmSetWindowAttribute,
        DwmUnregisterThumbnail, DwmUpdateThumbnailProperties,
    };
    use windows::Win32::Graphics::Gdi::{
        CreateRoundRectRgn, GetDC, GetMonitorInfoW, GetPixel, MONITOR_DEFAULTTOPRIMARY,
        MONITORINFO, MonitorFromWindow, ReleaseDC, SetWindowRgn,
    };
    use windows::Win32::UI::Input::KeyboardAndMouse::{
        GetAsyncKeyState, MOD_ALT, MOD_CONTROL, RegisterHotKey, VK_RCONTROL, VK_SPACE,
    };
    use windows::Win32::UI::WindowsAndMessaging::{
        CreateWindowExW, GW_HWNDNEXT, GWL_EXSTYLE, GWL_STYLE, GetMessageW, GetWindow,
        GetWindowLongPtrW, GetWindowRect, HWND_TOPMOST, IsWindowVisible, MSG, SW_HIDE,
        SW_SHOWNOACTIVATE, SWP_FRAMECHANGED, SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOSIZE,
        SetWindowLongPtrW, SetWindowPos, ShowWindow, WM_HOTKEY, WS_BORDER, WS_DISABLED,
        WS_DLGFRAME, WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW, WS_EX_TRANSPARENT, WS_POPUP,
        WS_THICKFRAME,
    };
    use windows::core::{BOOL, w};

    const HOTKEY_ID: i32 = 0xDB01;
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    const HOLD_THRESHOLD: Duration = Duration::from_millis(420);
    const OPTIMIZING_DURATION: Duration = Duration::from_millis(2_400);
    const BACKDROP_SAMPLES: [(i32, i32, u8); 5] = [
        (0, 0, 255),
        (-2, 0, 18),
        (2, 0, 18),
        (0, -1, 14),
        (0, 1, 14),
    ];
    static BACKDROP_HWND: AtomicIsize = AtomicIsize::new(0);
    static VOICE_CLIENT: Mutex<Option<VoiceClientProcess>> = Mutex::new(None);

    struct VoiceClientProcess {
        child: Child,
        stdin: ChildStdin,
    }

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
        suppress_native_frame(hwnd);
        clip_overlay_to_capsule(hwnd);
        let backdrop = create_backdrop_window(hwnd);
        BACKDROP_HWND.store(backdrop.0 as isize, Ordering::Release);
        hide_overlay(hwnd);
        hide_backdrop(backdrop);
        start_backdrop_thread(hwnd.0 as isize, backdrop.0 as isize);
        start_hotkey_thread(hwnd.0 as isize);
        start_hold_key_thread(hwnd.0 as isize);
    }

    fn create_backdrop_window(overlay: HWND) -> HWND {
        let capsule = capsule_screen_rect(overlay).expect("missing overlay bounds");
        let width = capsule.right - capsule.left;
        let height = capsule.bottom - capsule.top;
        unsafe {
            let backdrop = CreateWindowExW(
                WS_EX_NOACTIVATE | WS_EX_TOOLWINDOW | WS_EX_TRANSPARENT,
                w!("STATIC"),
                w!(""),
                WS_POPUP | WS_DISABLED,
                capsule.left,
                capsule.top,
                width,
                height,
                None,
                None,
                None,
                None,
            )
            .expect("failed to create DWM backdrop window");
            let region = CreateRoundRectRgn(0, 0, width + 1, height + 1, height, height);
            let _ = SetWindowRgn(backdrop, Some(region), false);
            backdrop
        }
    }

    fn suppress_native_frame(hwnd: HWND) {
        unsafe {
            let border_color = DWMWA_COLOR_NONE;
            let _ = DwmSetWindowAttribute(
                hwnd,
                DWMWA_BORDER_COLOR,
                &border_color as *const _ as *const c_void,
                std::mem::size_of_val(&border_color) as u32,
            );

            let _ = DwmSetWindowAttribute(
                hwnd,
                DWMWA_NCRENDERING_POLICY,
                &DWMNCRP_DISABLED as *const _ as *const c_void,
                std::mem::size_of_val(&DWMNCRP_DISABLED) as u32,
            );
        }
    }

    fn clip_overlay_to_capsule(hwnd: HWND) {
        unsafe {
            let mut window = RECT::default();
            if GetWindowRect(hwnd, &mut window).is_err() {
                return;
            }

            let window_width = window.right - window.left;
            let window_height = window.bottom - window.top;
            let scale_x = window_width as f32 / OVERLAY_WIDTH;
            let scale_y = window_height as f32 / OVERLAY_HEIGHT;
            let width = (LISTENING_CAPSULE_WIDTH * scale_x).round() as i32;
            let height = (LISTENING_CAPSULE_HEIGHT * scale_y).round() as i32;
            let left = (window_width - width) / 2;
            let top = (window_height - height) / 2;
            let region = CreateRoundRectRgn(
                left,
                top,
                left + width + 1,
                top + height + 1,
                height,
                height,
            );
            let _ = SetWindowRgn(hwnd, Some(region), true);
        }
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
            let backdrop = backdrop_hwnd(hwnd);
            hide_backdrop(backdrop);
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
            hide_backdrop(backdrop_hwnd(hwnd));
        }
    }

    fn begin_input(hwnd: HWND) {
        next_overlay_generation();
        set_dark_background(sample_dark_background(hwnd));
        show_phase(hwnd, OverlayPhase::Listening);
        if let Err(error) = start_voice_client() {
            append_voice_client_log(&format!("[gpui] failed to start voice client: {error}\n"));
        }
    }

    fn project_root() -> Option<PathBuf> {
        let executable = std::env::current_exe().ok()?;
        let mut directory = executable.parent()?.to_path_buf();
        for _ in 0..4 {
            if !directory.pop() {
                return None;
            }
        }
        Some(directory)
    }

    fn voice_client_log_path() -> PathBuf {
        std::env::temp_dir().join("doubao-gpui-voice-client.log")
    }

    fn append_voice_client_log(message: &str) {
        if let Ok(mut log) = OpenOptions::new()
            .create(true)
            .append(true)
            .open(voice_client_log_path())
        {
            let _ = log.write_all(message.as_bytes());
        }
    }

    fn start_voice_client() -> Result<(), String> {
        let mut active = VOICE_CLIENT
            .lock()
            .map_err(|_| "voice client lock is poisoned".to_string())?;
        if let Some(process) = active.as_mut() {
            match process.child.try_wait() {
                Ok(None) => return Ok(()),
                Ok(Some(_)) => {}
                Err(error) => return Err(format!("could not inspect previous client: {error}")),
            }
        }
        active.take();

        let root = project_root().ok_or_else(|| "could not locate project root".to_string())?;
        let script = root.join("clients").join("doubao_remote.py");
        if !script.is_file() {
            return Err(format!("missing client script: {}", script.display()));
        }

        let server = std::env::var("DOUBAO_BRIDGE_SERVER")
            .unwrap_or_else(|_| "100.116.241.81:4387".to_string());
        let audio_host = server
            .rsplit_once(':')
            .map(|(host, _)| host)
            .filter(|host| !host.is_empty())
            .ok_or_else(|| format!("invalid DOUBAO_BRIDGE_SERVER: {server}"))?;

        let log = OpenOptions::new()
            .create(true)
            .write(true)
            .truncate(true)
            .open(voice_client_log_path())
            .map_err(|error| format!("could not open voice client log: {error}"))?;
        let error_log = log
            .try_clone()
            .map_err(|error| format!("could not clone voice client log: {error}"))?;

        let mut child = Command::new("py");
        child
            .arg("-3")
            .arg(script)
            .arg("--server")
            .arg(&server)
            .arg("--udp-host")
            .arg(audio_host)
            .arg("--udp-port")
            .arg("5004")
            .arg("--audio-transport")
            .arg("tcp")
            .arg("record")
            .arg("--audio-start-delay")
            .arg("0.2")
            .arg("--audio-stop-delay")
            .arg("0.5")
            .arg("--recording-timeout")
            .arg("15")
            .arg("--final-timeout")
            .arg("8")
            .arg("--stdin-stop")
            .arg("--paste")
            .stdin(Stdio::piped())
            .stdout(Stdio::from(log))
            .stderr(Stdio::from(error_log))
            .creation_flags(CREATE_NO_WINDOW);

        let mut child = child
            .spawn()
            .map_err(|error| format!("could not launch Python client: {error}"))?;
        let stdin = child
            .stdin
            .take()
            .ok_or_else(|| "Python client stdin was not piped".to_string())?;
        *active = Some(VoiceClientProcess { child, stdin });
        Ok(())
    }

    fn stop_voice_client() -> Option<Child> {
        let mut active = VOICE_CLIENT.lock().ok()?;
        let mut process = active.take()?;
        let _ = process.stdin.write_all(b"stop\n");
        let _ = process.stdin.flush();
        drop(process.stdin);
        Some(process.child)
    }

    fn sample_dark_background(hwnd: HWND) -> bool {
        let Some(capsule) = capsule_screen_rect(hwnd) else {
            return false;
        };

        unsafe {
            let screen = GetDC(None);
            if screen.is_invalid() {
                return false;
            }

            let width = capsule.right - capsule.left;
            let height = capsule.bottom - capsule.top;
            let mut luminance_sum = 0u32;
            let mut dark_samples = 0u32;
            let mut valid_samples = 0u32;

            for x_step in 1..=7 {
                for y_step in 1..=3 {
                    let x = capsule.left + width * x_step / 8;
                    let y = capsule.top + height * y_step / 4;
                    let color = GetPixel(screen, x, y).0;
                    if color == u32::MAX {
                        continue;
                    }

                    let red = color & 0xff;
                    let green = (color >> 8) & 0xff;
                    let blue = (color >> 16) & 0xff;
                    let luminance = (red * 54 + green * 183 + blue * 19) / 256;
                    luminance_sum += luminance;
                    dark_samples += u32::from(luminance < 148);
                    valid_samples += 1;
                }
            }

            let _ = ReleaseDC(None, screen);
            if valid_samples == 0 {
                return false;
            }

            let average_luminance = luminance_sum / valid_samples;
            average_luminance < 150 || dark_samples * 2 >= valid_samples
        }
    }

    fn capsule_screen_rect(hwnd: HWND) -> Option<RECT> {
        let mut window = RECT::default();
        if unsafe { GetWindowRect(hwnd, &mut window) }.is_err() {
            return None;
        }
        let window_width = window.right - window.left;
        let window_height = window.bottom - window.top;
        let scale_x = window_width as f32 / OVERLAY_WIDTH;
        let scale_y = window_height as f32 / OVERLAY_HEIGHT;
        let width = (LISTENING_CAPSULE_WIDTH * scale_x).round() as i32;
        let height = (LISTENING_CAPSULE_HEIGHT * scale_y).round() as i32;
        let left = window.left + (window_width - width) / 2;
        let top = window.top + (window_height - height) / 2;
        Some(RECT {
            left,
            top,
            right: left + width,
            bottom: top + height,
        })
    }

    fn backdrop_hwnd(_overlay: HWND) -> HWND {
        HWND(BACKDROP_HWND.load(Ordering::Acquire) as *mut c_void)
    }

    fn hide_backdrop(backdrop: HWND) {
        unsafe {
            let _ = ShowWindow(backdrop, SW_HIDE);
        }
    }

    fn show_backdrop_below_overlay(backdrop: HWND, overlay: HWND) {
        unsafe {
            let _ = ShowWindow(backdrop, SW_SHOWNOACTIVATE);
            let _ = SetWindowPos(
                backdrop,
                Some(HWND_TOPMOST),
                0,
                0,
                0,
                0,
                SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE,
            );
            let _ = SetWindowPos(
                overlay,
                Some(HWND_TOPMOST),
                0,
                0,
                0,
                0,
                SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE,
            );
        }
    }

    fn start_backdrop_thread(overlay_value: isize, backdrop_value: isize) {
        thread::spawn(move || unsafe {
            let overlay = HWND(overlay_value as *mut c_void);
            let backdrop = HWND(backdrop_value as *mut c_void);
            let mut source = HWND::default();
            let mut thumbnails: Option<Vec<isize>> = None;

            loop {
                if IsWindowVisible(overlay).as_bool() {
                    let window_below = window_beneath_capsule(overlay, backdrop);
                    let binding_is_current = thumbnails
                        .as_ref()
                        .is_some_and(|handles| update_live_backdrop(handles, backdrop, source));
                    if window_below != source || !binding_is_current {
                        hide_backdrop(backdrop);
                        if let Some(handles) = thumbnails.take() {
                            unregister_live_backdrop(handles);
                        }
                        thumbnails = register_live_backdrop(backdrop, window_below);
                        source = if thumbnails.is_some() {
                            window_below
                        } else {
                            HWND::default()
                        };
                    }
                    if thumbnails.is_some() && !IsWindowVisible(backdrop).as_bool() {
                        show_backdrop_below_overlay(backdrop, overlay);
                    }
                } else if IsWindowVisible(backdrop).as_bool() {
                    hide_backdrop(backdrop);
                }
                thread::sleep(Duration::from_millis(50));
            }
        });
    }

    fn window_beneath_capsule(overlay: HWND, backdrop: HWND) -> HWND {
        unsafe {
            let mut capsule = RECT::default();
            if GetWindowRect(backdrop, &mut capsule).is_err() {
                return HWND::default();
            }
            let center_x = (capsule.left + capsule.right) / 2;
            let center_y = (capsule.top + capsule.bottom) / 2;
            let mut candidate = GetWindow(overlay, GW_HWNDNEXT).unwrap_or_default();

            while !candidate.is_invalid() {
                if candidate != overlay
                    && candidate != backdrop
                    && IsWindowVisible(candidate).as_bool()
                {
                    let mut bounds = RECT::default();
                    if GetWindowRect(candidate, &mut bounds).is_ok()
                        && center_x >= bounds.left
                        && center_x < bounds.right
                        && center_y >= bounds.top
                        && center_y < bounds.bottom
                    {
                        return candidate;
                    }
                }
                candidate = GetWindow(candidate, GW_HWNDNEXT).unwrap_or_default();
            }
            HWND::default()
        }
    }

    fn register_live_backdrop(backdrop: HWND, source: HWND) -> Option<Vec<isize>> {
        unsafe {
            if source.is_invalid() {
                return None;
            }

            let mut thumbnails = Vec::with_capacity(BACKDROP_SAMPLES.len());
            for _ in BACKDROP_SAMPLES {
                let Ok(thumbnail) = DwmRegisterThumbnail(backdrop, source) else {
                    unregister_live_backdrop(thumbnails);
                    return None;
                };
                thumbnails.push(thumbnail);
            }

            if !update_live_backdrop(&thumbnails, backdrop, source) {
                unregister_live_backdrop(thumbnails);
                return None;
            }
            Some(thumbnails)
        }
    }

    fn unregister_live_backdrop(thumbnails: Vec<isize>) {
        unsafe {
            for thumbnail in thumbnails {
                let _ = DwmUnregisterThumbnail(thumbnail);
            }
        }
    }

    fn update_live_backdrop(thumbnails: &[isize], backdrop: HWND, source: HWND) -> bool {
        unsafe {
            if thumbnails.len() != BACKDROP_SAMPLES.len() {
                return false;
            }

            let mut source_window = RECT::default();
            let mut backdrop_window = RECT::default();
            if GetWindowRect(source, &mut source_window).is_err()
                || GetWindowRect(backdrop, &mut backdrop_window).is_err()
            {
                return false;
            }

            if backdrop_window.left < source_window.left
                || backdrop_window.top < source_window.top
                || backdrop_window.right > source_window.right
                || backdrop_window.bottom > source_window.bottom
            {
                return false;
            }

            let destination_width = backdrop_window.right - backdrop_window.left;
            let destination_height = backdrop_window.bottom - backdrop_window.top;
            let inset_x = (destination_width / 24).max(3);
            let inset_y = (destination_height / 8).max(1);
            let source_left = backdrop_window.left - source_window.left + inset_x;
            let source_top = backdrop_window.top - source_window.top + inset_y;
            let source_width = destination_width - inset_x * 2;
            let source_height = destination_height - inset_y * 2;
            let source_window_width = source_window.right - source_window.left;
            let source_window_height = source_window.bottom - source_window.top;

            thumbnails.iter().zip(BACKDROP_SAMPLES).enumerate().all(
                |(index, (thumbnail, (offset_x, offset_y, light_opacity)))| {
                    let sample_left = source_left + offset_x;
                    let sample_top = source_top + offset_y;
                    if sample_left < 0
                        || sample_top < 0
                        || sample_left + source_width > source_window_width
                        || sample_top + source_height > source_window_height
                    {
                        return false;
                    }

                    let opacity = if index == 0 {
                        255
                    } else if dark_background() {
                        52
                    } else {
                        light_opacity
                    };
                    let properties = DWM_THUMBNAIL_PROPERTIES {
                        dwFlags: DWM_TNP_RECTDESTINATION
                            | DWM_TNP_RECTSOURCE
                            | DWM_TNP_OPACITY
                            | DWM_TNP_VISIBLE
                            | DWM_TNP_SOURCECLIENTAREAONLY,
                        rcDestination: RECT {
                            left: 0,
                            top: 0,
                            right: destination_width,
                            bottom: destination_height,
                        },
                        rcSource: RECT {
                            left: sample_left,
                            top: sample_top,
                            right: sample_left + source_width,
                            bottom: sample_top + source_height,
                        },
                        opacity,
                        fVisible: BOOL(1),
                        fSourceClientAreaOnly: BOOL(0),
                    };
                    DwmUpdateThumbnailProperties(*thumbnail, &properties).is_ok()
                },
            )
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
        if let Some(mut client) = stop_voice_client() {
            thread::spawn(move || {
                let _ = client.wait();
                if overlay_generation() == generation && overlay_phase() == OverlayPhase::Optimizing
                {
                    hide_overlay(HWND(hwnd_value as *mut c_void));
                }
            });
        } else {
            thread::spawn(move || {
                thread::sleep(OPTIMIZING_DURATION);
                if overlay_generation() == generation && overlay_phase() == OverlayPhase::Optimizing
                {
                    hide_overlay(HWND(hwnd_value as *mut c_void));
                }
            });
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
