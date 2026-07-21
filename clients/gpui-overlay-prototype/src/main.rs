#![cfg_attr(
    all(target_os = "windows", not(debug_assertions)),
    windows_subsystem = "windows"
)]

use std::{
    sync::atomic::{AtomicBool, AtomicU8, AtomicU32, AtomicU64, Ordering},
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use gpui::{
    Animation, AnimationExt as _, App, Application, Bounds, ColorSpace, Context, FontWeight,
    Window, WindowBackgroundAppearance, WindowBounds, WindowKind, WindowOptions, canvas, div, fill,
    linear_color_stop, linear_gradient, point, prelude::*, px, rgb, rgba, size,
};

#[cfg(any(target_os = "windows", target_os = "linux", test))]
mod client_core;
#[cfg(any(target_os = "windows", target_os = "linux", test))]
mod client_settings;
#[cfg(any(target_os = "windows", target_os = "linux"))]
mod native_voice;
#[cfg(any(target_os = "windows", target_os = "linux"))]
mod platform_paste;
#[cfg(target_os = "windows")]
mod windows_shell;

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
const BAR_AMPLITUDES: [f32; BAR_COUNT] = [
    0.24, 0.32, 0.44, 0.58, 0.42, 0.64, 0.88, 1.0, 0.78, 0.56, 0.92, 0.74, 0.58, 0.68, 0.49, 0.42,
    0.35, 0.3, 0.25, 0.2,
];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
enum OverlayPhase {
    Hidden,
    Activating,
    Listening,
    Optimizing,
}

static OVERLAY_PHASE: AtomicU8 = AtomicU8::new(OverlayPhase::Hidden as u8);
#[cfg(any(target_os = "windows", target_os = "linux"))]
static OVERLAY_GENERATION: AtomicU64 = AtomicU64::new(0);
static DARK_BACKGROUND: AtomicBool = AtomicBool::new(false);
static VOICE_ACTIVITY: AtomicU32 = AtomicU32::new(0);
static VOICE_ACTIVITY_UPDATED_AT: AtomicU64 = AtomicU64::new(0);
static SPEECH_ACTIVITY_UNTIL: AtomicU64 = AtomicU64::new(0);

#[cfg(any(target_os = "windows", target_os = "linux", test))]
const VOICE_RMS_GATE_DBFS: f32 = -58.0;
#[cfg(any(target_os = "windows", target_os = "linux", test))]
const VOICE_PEAK_GATE_DBFS: f32 = -45.0;
#[cfg(any(target_os = "windows", target_os = "linux", test))]
const VOICE_ONSET_RMS_DBFS: f32 = -40.0;
#[cfg(any(target_os = "windows", target_os = "linux", test))]
const VOICE_ONSET_PEAK_DBFS: f32 = -25.0;
const VOICE_ACTIVITY_STALE_AFTER_MS: u64 = 300;
#[cfg(any(target_os = "windows", target_os = "linux"))]
const SPEECH_ACTIVITY_HOLD_MS: u64 = 500;

fn overlay_phase() -> OverlayPhase {
    match OVERLAY_PHASE.load(Ordering::Acquire) {
        1 => OverlayPhase::Activating,
        2 => OverlayPhase::Listening,
        3 => OverlayPhase::Optimizing,
        _ => OverlayPhase::Hidden,
    }
}

fn set_overlay_phase(phase: OverlayPhase) {
    OVERLAY_PHASE.store(phase as u8, Ordering::Release);
}

#[cfg(any(target_os = "windows", target_os = "linux"))]
fn next_overlay_generation() -> u64 {
    OVERLAY_GENERATION.fetch_add(1, Ordering::AcqRel) + 1
}

#[cfg(any(target_os = "windows", target_os = "linux"))]
fn overlay_generation() -> u64 {
    OVERLAY_GENERATION.load(Ordering::Acquire)
}

fn dark_background() -> bool {
    DARK_BACKGROUND.load(Ordering::Acquire)
}

#[cfg(target_os = "windows")]
fn set_dark_background(dark: bool) {
    DARK_BACKGROUND.store(dark, Ordering::Release);
}

fn now_millis() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

#[cfg(any(target_os = "windows", target_os = "linux"))]
fn set_voice_activity(level: f32) {
    let quantized = (level.clamp(0.0, 1.0) * 1_000.0).round() as u32;
    VOICE_ACTIVITY.store(quantized, Ordering::Release);
    VOICE_ACTIVITY_UPDATED_AT.store(now_millis(), Ordering::Release);
}

#[cfg(any(target_os = "windows", target_os = "linux"))]
fn mark_speech_activity() {
    SPEECH_ACTIVITY_UNTIL.store(
        now_millis().saturating_add(SPEECH_ACTIVITY_HOLD_MS),
        Ordering::Release,
    );
}

#[cfg(any(target_os = "windows", target_os = "linux"))]
fn clear_voice_activity() {
    set_voice_activity(0.0);
    SPEECH_ACTIVITY_UNTIL.store(0, Ordering::Release);
}

fn decoded_voice_activity(level: u32, updated_at: u64, speech_until: u64, now: u64) -> f32 {
    if level == 0
        || now > speech_until
        || now.saturating_sub(updated_at) > VOICE_ACTIVITY_STALE_AFTER_MS
    {
        return 0.0;
    }
    level as f32 / 1_000.0
}

fn voice_activity() -> f32 {
    decoded_voice_activity(
        VOICE_ACTIVITY.load(Ordering::Acquire),
        VOICE_ACTIVITY_UPDATED_AT.load(Ordering::Acquire),
        SPEECH_ACTIVITY_UNTIL.load(Ordering::Acquire),
        now_millis(),
    )
}

#[cfg(test)]
fn parse_dbfs(line: &str, label: &str) -> Option<f32> {
    let value = line.split_once(label)?.1.split_once("dBFS")?.0.trim();
    value.parse().ok()
}

#[cfg(test)]
fn voice_activity_from_audio_line(line: &str) -> Option<f32> {
    if !line.starts_with("[local_audio_level]") {
        return None;
    }
    let rms = parse_dbfs(line, "rms=")?;
    let peak = parse_dbfs(line, "peak=")?;
    Some(voice_activity_from_levels(rms, peak))
}

#[cfg(any(target_os = "windows", target_os = "linux", test))]
fn voice_activity_from_levels(rms: f32, peak: f32) -> f32 {
    if rms < VOICE_RMS_GATE_DBFS || peak < VOICE_PEAK_GATE_DBFS {
        return 0.0;
    }

    let rms_strength = ((rms - VOICE_RMS_GATE_DBFS) / -VOICE_RMS_GATE_DBFS).clamp(0.0, 1.0);
    let peak_strength = ((peak - VOICE_PEAK_GATE_DBFS) / -VOICE_PEAK_GATE_DBFS).clamp(0.0, 1.0);
    (0.75 * rms_strength + 0.25 * peak_strength).clamp(0.12, 1.0)
}

#[cfg(any(target_os = "windows", target_os = "linux"))]
enum NativeVoiceEventOutcome {
    Continue,
    Error(String),
    Finished,
}

#[cfg(any(target_os = "windows", target_os = "linux", test))]
fn overlay_phase_for_bridge_phase(phase: &str) -> Option<OverlayPhase> {
    match phase {
        "arming" | "voice_retry" => Some(OverlayPhase::Activating),
        "recording" => Some(OverlayPhase::Listening),
        "optimizing" => Some(OverlayPhase::Optimizing),
        _ => None,
    }
}

#[cfg(any(target_os = "windows", target_os = "linux"))]
fn apply_native_voice_event(event: native_voice::NativeVoiceEvent) -> NativeVoiceEventOutcome {
    use native_voice::NativeVoiceEvent;

    match event {
        NativeVoiceEvent::Phase(phase) => {
            if let Some(phase) = overlay_phase_for_bridge_phase(&phase) {
                if phase == OverlayPhase::Listening {
                    clear_voice_activity();
                }
                set_overlay_phase(phase);
            }
            NativeVoiceEventOutcome::Continue
        }
        NativeVoiceEvent::Partial(text) => {
            if !text.trim().is_empty() {
                mark_speech_activity();
            }
            NativeVoiceEventOutcome::Continue
        }
        NativeVoiceEvent::AudioLevel {
            rms_dbfs,
            peak_dbfs,
        } => {
            set_voice_activity(voice_activity_from_levels(rms_dbfs, peak_dbfs));
            if rms_dbfs >= VOICE_ONSET_RMS_DBFS && peak_dbfs >= VOICE_ONSET_PEAK_DBFS {
                mark_speech_activity();
            }
            NativeVoiceEventOutcome::Continue
        }
        NativeVoiceEvent::Error(error) => NativeVoiceEventOutcome::Error(error),
        NativeVoiceEvent::Finished => {
            clear_voice_activity();
            NativeVoiceEventOutcome::Finished
        }
    }
}

#[cfg(test)]
fn is_strong_voice_activity_line(line: &str) -> bool {
    let Some(rms) = parse_dbfs(line, "rms=") else {
        return false;
    };
    let Some(peak) = parse_dbfs(line, "peak=") else {
        return false;
    };
    rms >= VOICE_ONSET_RMS_DBFS && peak >= VOICE_ONSET_PEAK_DBFS
}

#[cfg(test)]
fn is_partial_speech_line(line: &str) -> bool {
    line.strip_prefix("[partial]")
        .is_some_and(|text| !text.trim().is_empty())
}

#[cfg(test)]
fn overlay_phase_from_bridge_line(line: &str) -> Option<OverlayPhase> {
    overlay_phase_for_bridge_phase(line.strip_prefix("[bridge_phase] ")?.trim())
}

fn waveform_bar_height(index: usize, delta: f32, voice_level: f32) -> f32 {
    if voice_level == 0.0 {
        return 3.0;
    }

    let phase = delta * std::f32::consts::TAU;
    let offset = index as f32 * 0.53;
    let random_phase = ((index * 73 + 19) % 101) as f32 / 101.0 * std::f32::consts::TAU;
    let primary =
        ((phase * (0.82 + index as f32 * 0.013) + offset + random_phase).sin() + 1.0) * 0.5;
    let secondary = ((phase * 2.17 - offset * 0.71 + random_phase * 0.37).sin() + 1.0) * 0.5;
    let movement = 0.62 * primary + 0.38 * secondary;
    3.0 + 13.0 * BAR_AMPLITUDES[index] * voice_level.sqrt() * (0.24 + 0.76 * movement)
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
    canvas(
        |_, _, _| {},
        move |bounds, _, window, _| {
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
            let voice_level = voice_activity();

            for index in 0..BAR_COUNT {
                let height = waveform_bar_height(index, delta, voice_level);
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

fn activating_capsule(delta: f32) -> impl IntoElement {
    capsule_base()
        .id("voice-capsule")
        .relative()
        .w(px(LISTENING_CAPSULE_WIDTH))
        .h(px(LISTENING_CAPSULE_HEIGHT))
        .cursor_pointer()
        .on_click(|_, window, _| platform::finish_input(window))
        .child(glass_canvas(delta, false).absolute().top_0().left_0())
        .child(
            div()
                .relative()
                .text_xs()
                .font_weight(FontWeight::MEDIUM)
                .child("激活中"),
        )
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
                        OverlayPhase::Activating => activating_capsule(delta).into_any_element(),
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
    #[cfg(target_os = "windows")]
    if !platform::claim_single_instance() {
        return;
    }

    Application::new().run(|cx: &mut App| {
        let options = WindowOptions {
            window_bounds: Some(WindowBounds::Windowed(overlay_bounds(cx))),
            titlebar: None,
            focus: cfg!(not(target_os = "windows"))
                && std::env::var_os("DOUBAO_OVERLAY_FOCUS").is_some(),
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
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicIsize, Ordering};
    use std::thread;
    use std::time::{Duration, Instant};

    use super::native_voice::{NativeVoiceConfig, NativeVoiceController};
    use super::platform_paste::PasteTarget;
    use super::{
        LISTENING_CAPSULE_HEIGHT, LISTENING_CAPSULE_WIDTH, NativeVoiceEventOutcome, OVERLAY_HEIGHT,
        OVERLAY_WIDTH, OverlayPhase, apply_native_voice_event, clear_voice_activity,
        dark_background, next_overlay_generation, overlay_generation, overlay_phase,
        set_dark_background, set_overlay_phase,
    };
    use gpui::Window;
    use raw_window_handle::{HasWindowHandle, RawWindowHandle};
    use windows::Win32::Foundation::{
        ERROR_ALREADY_EXISTS, GetLastError, HWND, LPARAM, RECT, WPARAM,
    };
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
    use windows::Win32::System::Threading::CreateMutexW;
    use windows::Win32::UI::Input::KeyboardAndMouse::{
        GetAsyncKeyState, MOD_ALT, MOD_CONTROL, RegisterHotKey, VK_LCONTROL, VK_SPACE,
    };
    use windows::Win32::UI::WindowsAndMessaging::{
        CreateWindowExW, FindWindowW, GW_HWNDNEXT, GWL_EXSTYLE, GWL_STYLE, GetMessageW, GetWindow,
        GetWindowLongPtrW, GetWindowRect, HWND_TOPMOST, IsWindowVisible, MSG, SW_HIDE, SW_SHOW,
        SW_SHOWNOACTIVATE, SWP_FRAMECHANGED, SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOSIZE,
        SetForegroundWindow, SetWindowLongPtrW, SetWindowPos, ShowWindow, WM_HOTKEY, WS_BORDER,
        WS_DISABLED, WS_DLGFRAME, WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW, WS_EX_TRANSPARENT, WS_POPUP,
        WS_THICKFRAME,
    };
    use windows::core::{BOOL, w};

    const HOTKEY_ID: i32 = 0xDB01;
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
    static INSTANCE_MUTEX: AtomicIsize = AtomicIsize::new(0);
    static VOICE_CLIENT: NativeVoiceController = NativeVoiceController::new();

    pub fn claim_single_instance() -> bool {
        let Ok(handle) =
            (unsafe { CreateMutexW(None, false, w!("Local\\DoubaoVoiceClient.SingleInstance")) })
        else {
            return true;
        };
        if unsafe { GetLastError() } == ERROR_ALREADY_EXISTS {
            if let Ok(settings) =
                unsafe { FindWindowW(w!("DoubaoVoiceClientSettings"), w!("Doubao Voice Client")) }
            {
                unsafe {
                    let _ = ShowWindow(settings, SW_SHOW);
                    let _ = SetForegroundWindow(settings);
                }
            }
            return false;
        }
        INSTANCE_MUTEX.store(handle.0 as isize, Ordering::Release);
        true
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
        super::windows_shell::start(hwnd);
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
        clear_voice_activity();
        set_overlay_phase(OverlayPhase::Hidden);
        unsafe {
            let _ = ShowWindow(hwnd, SW_HIDE);
            hide_backdrop(backdrop_hwnd(hwnd));
        }
    }

    fn begin_input(hwnd: HWND) {
        let generation = next_overlay_generation();
        clear_voice_activity();
        set_dark_background(sample_dark_background(hwnd));
        show_phase(hwnd, OverlayPhase::Activating);
        if let Err(error) = start_voice_client(generation, hwnd) {
            append_voice_client_log(&format!("[gpui] failed to start voice client: {error}\n"));
        }
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

    fn start_voice_client(generation: u64, hwnd: HWND) -> Result<(), String> {
        OpenOptions::new()
            .create(true)
            .write(true)
            .truncate(true)
            .open(voice_client_log_path())
            .map_err(|error| format!("could not open voice client log: {error}"))?;
        let config =
            NativeVoiceConfig::from_environment()?.with_paste_target(PasteTarget::for_window(hwnd));
        let hwnd_value = hwnd.0 as isize;
        VOICE_CLIENT
            .start(config, move |event| {
                if overlay_generation() != generation {
                    return;
                }
                match apply_native_voice_event(event) {
                    NativeVoiceEventOutcome::Continue => {}
                    NativeVoiceEventOutcome::Error(error) => {
                        append_voice_client_log(&format!("[native-client] {error}\n"));
                    }
                    NativeVoiceEventOutcome::Finished => {
                        hide_overlay(HWND(hwnd_value as *mut c_void));
                    }
                }
            })
            .map(|_| ())
    }

    fn stop_voice_client() -> bool {
        VOICE_CLIENT.request_stop()
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
            OverlayPhase::Activating | OverlayPhase::Listening => {}
        }

        let generation = overlay_generation();
        clear_voice_activity();
        show_phase(hwnd, OverlayPhase::Optimizing);
        let hwnd_value = hwnd.0 as isize;
        if !stop_voice_client() {
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
                let is_down = unsafe { GetAsyncKeyState(VK_LCONTROL.0 as i32) < 0 };

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

#[cfg(test)]
mod tests {
    use super::{
        BAR_COUNT, OverlayPhase, decoded_voice_activity, is_partial_speech_line,
        is_strong_voice_activity_line, overlay_phase_from_bridge_line,
        voice_activity_from_audio_line, waveform_bar_height,
    };

    #[test]
    fn bridge_status_distinguishes_activation_from_recording() {
        assert_eq!(
            overlay_phase_from_bridge_line("[bridge_phase] arming"),
            Some(OverlayPhase::Activating)
        );
        assert_eq!(
            overlay_phase_from_bridge_line("[bridge_phase] voice_retry"),
            Some(OverlayPhase::Activating)
        );
        assert_eq!(
            overlay_phase_from_bridge_line("[bridge_phase] recording"),
            Some(OverlayPhase::Listening)
        );
        assert_eq!(
            overlay_phase_from_bridge_line("[bridge_phase] optimizing"),
            Some(OverlayPhase::Optimizing)
        );
        assert_eq!(
            overlay_phase_from_bridge_line("[event] {'phase': 'recording'}"),
            None
        );
    }

    #[test]
    fn silence_is_hard_gated_to_zero() {
        let level =
            voice_activity_from_audio_line("[local_audio_level] rms=-92.4dBFS peak=-80.8dBFS");

        assert_eq!(level, Some(0.0));
    }

    #[test]
    fn speech_level_crosses_the_activity_gate() {
        let level =
            voice_activity_from_audio_line("[local_audio_level] rms=-46.6dBFS peak=-36.4dBFS")
                .expect("audio level should parse");

        assert!(level > 0.12);
        assert!(level < 1.0);
    }

    #[test]
    fn stale_activity_is_forced_to_zero() {
        assert!(decoded_voice_activity(650, 1_000, 2_000, 1_200) > 0.0);
        assert_eq!(decoded_voice_activity(650, 1_000, 2_000, 1_301), 0.0);
    }

    #[test]
    fn energy_does_not_move_without_recognized_speech() {
        assert_eq!(decoded_voice_activity(650, 1_000, 0, 1_100), 0.0);
        assert!(is_partial_speech_line("[partial] 你好"));
        assert!(!is_partial_speech_line("[partial]   "));
    }

    #[test]
    fn silent_waveform_is_identical_at_every_animation_time() {
        for index in 0..BAR_COUNT {
            assert_eq!(
                waveform_bar_height(index, 0.1, 0.0),
                waveform_bar_height(index, 0.9, 0.0)
            );
        }
    }

    #[test]
    fn active_waveform_changes_with_animation_time() {
        let changed_bars = (0..BAR_COUNT)
            .filter(|index| {
                (waveform_bar_height(*index, 0.1, 0.7) - waveform_bar_height(*index, 0.4, 0.7))
                    .abs()
                    > 0.25
            })
            .count();

        assert!(changed_bars > BAR_COUNT / 2);
    }

    #[test]
    fn strong_voice_onset_ignores_the_observed_noise_floor() {
        assert!(!is_strong_voice_activity_line(
            "[local_audio_level] rms=-47.0dBFS peak=-35.0dBFS"
        ));
        assert!(is_strong_voice_activity_line(
            "[local_audio_level] rms=-20.0dBFS peak=-8.0dBFS"
        ));
    }
}

#[cfg(target_os = "linux")]
mod platform {
    use gpui::Window;

    use super::native_voice::{NativeVoiceConfig, NativeVoiceController, NativeVoiceEvent};
    use super::{
        NativeVoiceEventOutcome, OverlayPhase, apply_native_voice_event, clear_voice_activity,
        next_overlay_generation, overlay_generation, set_overlay_phase,
    };

    static VOICE_CLIENT: NativeVoiceController = NativeVoiceController::new();

    pub fn configure_overlay(_window: &Window) {
        if let Err(error) = ctrlc::set_handler(|| {
            if stop_voice_client() {
                set_overlay_phase(OverlayPhase::Optimizing);
            }
        }) {
            eprintln!("[linux-client] could not install signal handler: {error}");
        }
        clear_voice_activity();
        set_overlay_phase(OverlayPhase::Activating);
        if let Err(error) = start_voice_client() {
            eprintln!("[linux-client] failed to start voice client: {error}");
            set_overlay_phase(OverlayPhase::Optimizing);
        }
    }

    pub fn finish_input(window: &mut Window) {
        if stop_voice_client() {
            set_overlay_phase(OverlayPhase::Optimizing);
        } else {
            window.remove_window();
        }
    }

    fn start_voice_client() -> Result<(), String> {
        let generation = next_overlay_generation();
        let config = NativeVoiceConfig::from_environment()?;
        VOICE_CLIENT
            .start(config, move |event| {
                if overlay_generation() != generation {
                    return;
                }
                if let NativeVoiceEvent::Phase(phase) = &event {
                    eprintln!("[linux-client] phase={phase}");
                }
                match apply_native_voice_event(event) {
                    NativeVoiceEventOutcome::Continue => {}
                    NativeVoiceEventOutcome::Error(error) => {
                        eprintln!("[linux-client] {error}");
                        set_overlay_phase(OverlayPhase::Optimizing);
                    }
                    NativeVoiceEventOutcome::Finished => {
                        eprintln!("[linux-client] finished");
                        std::process::exit(0);
                    }
                }
            })
            .map(|_| ())
    }

    fn stop_voice_client() -> bool {
        VOICE_CLIENT.request_stop()
    }
}

#[cfg(not(any(target_os = "windows", target_os = "linux")))]
mod platform {
    use gpui::Window;

    use super::{OverlayPhase, set_overlay_phase};

    pub fn configure_overlay(_window: &Window) {
        set_overlay_phase(OverlayPhase::Listening);
    }

    pub fn finish_input(window: &mut Window) {
        window.remove_window();
    }
}
