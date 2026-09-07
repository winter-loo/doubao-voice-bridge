#![cfg_attr(
    all(target_os = "windows", not(debug_assertions)),
    windows_subsystem = "windows"
)]

use std::{
    sync::{
        Arc, Mutex, OnceLock,
        atomic::{AtomicBool, AtomicU8, AtomicU32, AtomicU64, Ordering},
    },
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use gpui::{
    Animation, AnimationExt as _, App, Application, Bounds, Context, Corners, FontWeight,
    RenderImage, Window, WindowBackgroundAppearance, WindowBounds, WindowKind, WindowOptions,
    canvas, div, fill, point, prelude::*, px, rgb, rgba, size,
};
#[cfg(target_os = "linux")]
use gpui::{ClipboardItem, Entity, Focusable, KeyBinding};
use image::{Frame, RgbaImage};

#[cfg(any(target_os = "windows", target_os = "linux", test))]
mod client_core;
#[cfg(any(target_os = "windows", target_os = "linux", test))]
mod client_settings;
#[cfg(target_os = "linux")]
mod linux_global_shortcuts;
#[cfg(target_os = "linux")]
mod linux_gnome_shortcuts;
#[cfg(any(target_os = "linux", test))]
mod linux_shortcut;
#[cfg(target_os = "linux")]
mod linux_transcript_input;
#[cfg(target_os = "linux")]
mod linux_tray;
mod liquid_glass;
#[cfg(any(target_os = "windows", target_os = "linux"))]
mod native_voice;
#[cfg(any(target_os = "windows", target_os = "linux"))]
mod platform_paste;
#[cfg(target_os = "linux")]
mod settings_gui;
#[cfg(target_os = "linux")]
mod tray_icon;
#[cfg(target_os = "windows")]
mod windows_shell;

const CLIENT_APP_ID: &str = "local.doubao.voicebridge";
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
#[cfg(target_os = "linux")]
const TRANSCRIPT_WINDOW_WIDTH: f32 = 560.0;
#[cfg(target_os = "linux")]
const TRANSCRIPT_WINDOW_HEIGHT: f32 = 240.0;
#[cfg(target_os = "linux")]
const TRANSCRIPT_OVERLAY_GAP: f32 = 14.0;
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

#[cfg(any(target_os = "linux", test))]
#[derive(Clone, Debug, Default, PartialEq, Eq)]
struct VoiceTranscript {
    partial: String,
    committed: String,
    final_text: String,
}

#[cfg(any(target_os = "linux", test))]
impl VoiceTranscript {
    fn begin_session(&mut self) {
        self.partial.clear();
        self.committed.clear();
        self.final_text.clear();
    }

    fn partial(&mut self, text: String) {
        self.partial = text;
    }

    fn committed(&mut self, text: String) {
        self.committed = text;
        self.partial.clear();
    }

    fn final_text(&mut self, text: String) {
        self.final_text = text;
        self.partial.clear();
    }

    fn visible_text(&self) -> &str {
        if !self.final_text.is_empty() {
            &self.final_text
        } else if !self.partial.is_empty() {
            &self.partial
        } else {
            &self.committed
        }
    }
}

#[cfg(any(target_os = "linux", test))]
fn transcript_presentation(phase: OverlayPhase, text: &str) -> (&'static str, &str) {
    if text.is_empty() {
        let label = match phase {
            OverlayPhase::Activating => "正在连接",
            OverlayPhase::Listening => "正在聆听",
            OverlayPhase::Optimizing => "正在整理",
            OverlayPhase::Hidden => "语音输入",
        };
        (label, "识别到的文字会显示在这里")
    } else {
        let label = if phase == OverlayPhase::Optimizing {
            "识别结果"
        } else {
            "实时转写"
        };
        (label, text)
    }
}

#[cfg(target_os = "linux")]
fn transcript_window_should_be_visible(phase: OverlayPhase, session_completed: bool) -> bool {
    phase != OverlayPhase::Hidden || session_completed
}

#[cfg(target_os = "linux")]
fn transcript_window_state_atoms() -> [&'static str; 1] {
    ["_NET_WM_STATE_ABOVE"]
}

#[cfg(target_os = "linux")]
fn should_apply_voice_snapshot(last_applied: &str, next: &str) -> bool {
    last_applied != next
}

#[cfg(target_os = "linux")]
fn transcript_window_geometry(visible: bool, saved: (i32, i32, u32, u32)) -> (i32, i32, u32, u32) {
    if visible { saved } else { (-100, -100, 1, 1) }
}

#[cfg(target_os = "linux")]
fn transcript_geometry_above_overlay(overlay: (i32, i32, u32, u32)) -> (i32, i32, u32, u32) {
    let (overlay_x, overlay_y, overlay_width, _) = overlay;
    let width = TRANSCRIPT_WINDOW_WIDTH.round() as u32;
    let height = TRANSCRIPT_WINDOW_HEIGHT.round() as u32;
    let x = overlay_x + (overlay_width as i32 - width as i32) / 2;
    let y = overlay_y - height as i32 - TRANSCRIPT_OVERLAY_GAP.round() as i32;
    (x, y, width, height)
}

#[cfg(target_os = "linux")]
fn transcript_bootstrap_size() -> gpui::Size<gpui::Pixels> {
    size(px(1.0), px(1.0))
}

#[cfg(target_os = "linux")]
fn transcript_geometry_for_frame(
    geometry: (i32, i32, u32, u32),
    frame_extents: (u32, u32, u32, u32),
) -> (i32, i32, u32, u32) {
    (
        geometry.0 - frame_extents.0 as i32,
        geometry.1 - frame_extents.2 as i32,
        geometry.2,
        geometry.3,
    )
}

#[cfg(target_os = "linux")]
const fn net_wm_state_action(enabled: bool) -> u32 {
    if enabled { 1 } else { 0 }
}

#[cfg(target_os = "linux")]
const fn should_create_transcript_window(requested: bool, created: bool) -> bool {
    requested && !created
}

#[cfg(target_os = "linux")]
const fn transcript_state_after_user_close(_requested: bool, _created: bool) -> (bool, bool) {
    (false, false)
}

#[cfg(target_os = "linux")]
fn reset_transcript_window_state() {
    let (requested, created) = transcript_state_after_user_close(
        TRANSCRIPT_WINDOW_REQUESTED.load(Ordering::Acquire),
        TRANSCRIPT_WINDOW_CREATED.load(Ordering::Acquire),
    );
    TRANSCRIPT_WINDOW_REQUESTED.store(requested, Ordering::Release);
    TRANSCRIPT_WINDOW_CREATED.store(created, Ordering::Release);
    platform::forget_transcript_window();
}

#[cfg(target_os = "linux")]
static VOICE_TRANSCRIPT: OnceLock<Mutex<VoiceTranscript>> = OnceLock::new();
#[cfg(target_os = "linux")]
static TRANSCRIPT_WINDOW_REQUESTED: AtomicBool = AtomicBool::new(false);
#[cfg(target_os = "linux")]
static TRANSCRIPT_WINDOW_CREATED: AtomicBool = AtomicBool::new(false);

#[cfg(target_os = "linux")]
fn with_voice_transcript(update: impl FnOnce(&mut VoiceTranscript)) {
    let transcript = VOICE_TRANSCRIPT.get_or_init(|| Mutex::new(VoiceTranscript::default()));
    if let Ok(mut transcript) = transcript.lock() {
        update(&mut transcript);
    }
}

#[cfg(target_os = "linux")]
fn voice_transcript() -> VoiceTranscript {
    VOICE_TRANSCRIPT
        .get_or_init(|| Mutex::new(VoiceTranscript::default()))
        .lock()
        .map(|transcript| transcript.clone())
        .unwrap_or_default()
}

#[cfg(any(target_os = "linux", test))]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum VoiceHotkeyAction {
    Begin,
    Finish,
    Ignore,
}

#[cfg(any(target_os = "linux", test))]
fn voice_hotkey_action(phase: OverlayPhase) -> VoiceHotkeyAction {
    match phase {
        OverlayPhase::Hidden => VoiceHotkeyAction::Begin,
        OverlayPhase::Activating | OverlayPhase::Listening => VoiceHotkeyAction::Finish,
        OverlayPhase::Optimizing => VoiceHotkeyAction::Ignore,
    }
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
    #[cfg(target_os = "linux")]
    linux_tray::refresh();
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

/// Luminance below which a first look calls the background dark.
#[cfg(target_os = "windows")]
const FRESH_DARK_LUMINANCE: u32 = 150;
/// Thresholds for a background that is already being tracked. The gap between them is
/// hysteresis: once the palette has switched it takes a clearly different background to
/// switch it back, so a window edge parked under the capsule cannot make the glass
/// oscillate between palettes.
#[cfg(target_os = "windows")]
const ENTER_DARK_LUMINANCE: u32 = 132;
#[cfg(target_os = "windows")]
const LEAVE_DARK_LUMINANCE: u32 = 168;

/// Turns a ring of luminance probes into a palette choice.
///
/// Kept separate from the Win32 pixel reads so the thresholds can be exercised without
/// a screen. `previous` is the palette currently on display, or `None` for a first look:
/// a first look uses one threshold, a follow-up uses the hysteresis band.
#[cfg(target_os = "windows")]
fn decide_dark_background(
    average_luminance: u32,
    dark_samples: u32,
    valid_samples: u32,
    previous: Option<bool>,
) -> bool {
    if valid_samples == 0 {
        return previous.unwrap_or(false);
    }

    // A mostly dark ring wins outright, so a bright strip crossing one side cannot wash
    // out an otherwise dark background.
    if dark_samples * 2 >= valid_samples {
        return true;
    }

    let threshold = match previous {
        None => FRESH_DARK_LUMINANCE,
        Some(true) => LEAVE_DARK_LUMINANCE,
        Some(false) => ENTER_DARK_LUMINANCE,
    };
    average_luminance < threshold
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
        "arming" | "voice_retry" | "ui_ready" | "asr_warmup" => Some(OverlayPhase::Activating),
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
            #[cfg(target_os = "linux")]
            with_voice_transcript(|transcript| transcript.partial(text));
            NativeVoiceEventOutcome::Continue
        }
        NativeVoiceEvent::Committed(text) => {
            #[cfg(target_os = "linux")]
            with_voice_transcript(|transcript| transcript.committed(text));
            #[cfg(target_os = "windows")]
            let _ = text;
            NativeVoiceEventOutcome::Continue
        }
        NativeVoiceEvent::Final(text) => {
            #[cfg(target_os = "linux")]
            with_voice_transcript(|transcript| transcript.final_text(text));
            #[cfg(target_os = "windows")]
            let _ = text;
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

/// The generated glass textures, keyed by device-pixel size and lighting environment.
///
/// The optics are fixed for a given capsule; only the sheen phase moves, and every phase
/// is baked into one multi-frame image. Shading is cheap, but the atlas upload is not,
/// so regenerating this per frame would push a new tile sixty times a second for a
/// picture that never changes. There are at most a handful of keys -- one capsule size
/// per display scale, times light and dark -- so the cache never needs eviction.
fn glass_texture(width: u32, height: u32, dark: bool) -> Arc<RenderImage> {
    type Key = (u32, u32, bool);
    static CACHE: OnceLock<Mutex<Vec<(Key, Arc<RenderImage>)>>> = OnceLock::new();

    let key = (width, height, dark);
    let mut cache = CACHE
        .get_or_init(|| Mutex::new(Vec::new()))
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    if let Some((_, texture)) = cache.iter().find(|(cached, _)| *cached == key) {
        return texture.clone();
    }

    let frames = liquid_glass::CapsuleGlass::new(width, height, dark)
        .render_frames()
        .into_iter()
        .map(|pixels| {
            Frame::new(
                RgbaImage::from_raw(width, height, pixels)
                    .expect("a glass frame is exactly width * height * 4 bytes"),
            )
        })
        .collect::<Vec<_>>();
    let texture = Arc::new(RenderImage::new(frames));
    cache.push((key, texture.clone()));
    texture
}

fn glass_canvas(delta: f32, show_waveform: bool) -> impl IntoElement + Styled {
    canvas(
        |_, _, _| {},
        move |bounds, _, window, _| {
            let radius = bounds.size.height / 2.0;
            let scale = window.scale_factor();
            let width = (f32::from(bounds.size.width) * scale).round().max(1.0) as u32;
            let height = (f32::from(bounds.size.height) * scale).round().max(1.0) as u32;
            let sheen = ((delta * liquid_glass::SHEEN_FRAMES as f32) as usize)
                .min(liquid_glass::SHEEN_FRAMES - 1);
            let dark = dark_background();

            if window
                .paint_image(
                    bounds,
                    Corners::all(radius),
                    glass_texture(width, height, dark),
                    sheen,
                    false,
                )
                .is_err()
            {
                // The sprite atlas refused the tile. Fall back to a flat capsule so the
                // overlay stays readable instead of disappearing.
                let flat = if dark {
                    rgba(0x0c1a2ad8)
                } else {
                    rgba(0xffffffbc)
                };
                window.paint_quad(fill(bounds, flat).corner_radii(radius));
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

/// The capsule shell. It carries layout and text color only: the fill, the rim and the
/// outline all come out of the glass texture, which shapes them to the real silhouette
/// instead of to an axis-aligned box.
fn capsule_base() -> gpui::Div {
    div()
        .relative()
        .flex()
        .items_center()
        .justify_center()
        .rounded_full()
        .overflow_hidden()
        .text_color(if dark_background() {
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

#[cfg(target_os = "linux")]
struct TranscriptWindow {
    input: Entity<linux_transcript_input::TranscriptInput>,
    last_voice_text: String,
}

#[cfg(target_os = "linux")]
impl Render for TranscriptWindow {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        window.request_animation_frame();
        let phase = overlay_phase();
        let transcript = voice_transcript();
        let (label, text) = transcript_presentation(phase, transcript.visible_text());
        if should_apply_voice_snapshot(&self.last_voice_text, text) {
            self.last_voice_text = text.to_string();
            self.input.update(cx, |input, cx| {
                input.set_voice_text_and_notify(text, cx);
            });
        }
        let input_for_copy = self.input.clone();

        div()
            .size_full()
            .flex()
            .flex_col()
            .gap_3()
            .p_5()
            .bg(rgb(0x0b1120))
            .text_color(rgba(0xfffffff2))
            .child(
                div()
                    .text_lg()
                    .font_weight(FontWeight::SEMIBOLD)
                    .child("豆包语音文本"),
            )
            .child(
                div()
                    .w_full()
                    .flex()
                    .items_center()
                    .justify_between()
                    .child(
                        div()
                            .text_xs()
                            .font_weight(FontWeight::MEDIUM)
                            .text_color(rgba(0x7dd3fccc))
                            .child(label),
                    )
                    .child(
                        div()
                            .id("copy-transcript")
                            .px_3()
                            .py_1()
                            .rounded_md()
                            .bg(rgba(0x38bdf833))
                            .border_1()
                            .border_color(rgba(0x7dd3fc88))
                            .text_xs()
                            .text_color(rgba(0xdff7ffff))
                            .cursor_pointer()
                            .hover(|style| style.bg(rgba(0x38bdf855)))
                            .on_click(move |_, _, cx| {
                                let text = input_for_copy.read(cx).content().to_string();
                                cx.write_to_clipboard(ClipboardItem::new_string(text));
                            })
                            .child("复制"),
                    ),
            )
            .child(
                div()
                    .flex_1()
                    .w_full()
                    .px_4()
                    .py_3()
                    .rounded_lg()
                    .overflow_hidden()
                    .bg(rgb(0x111827))
                    .border_1()
                    .border_color(rgba(0xffffff24))
                    .text_color(rgba(0xfffffff2))
                    .child(self.input.clone()),
            )
    }
}

#[cfg(target_os = "linux")]
fn transcript_window_bounds(cx: &App) -> Bounds<gpui::Pixels> {
    let overlay = overlay_bounds(cx);
    let transcript_size = size(px(TRANSCRIPT_WINDOW_WIDTH), px(TRANSCRIPT_WINDOW_HEIGHT));
    Bounds {
        origin: point(
            overlay.origin.x + (overlay.size.width - transcript_size.width) / 2.0,
            overlay.origin.y - transcript_size.height - px(TRANSCRIPT_OVERLAY_GAP),
        ),
        // Mutter can place a normal window before our X11 correction arrives.
        // Make that first mapped frame visually empty, then expand it in place.
        size: transcript_bootstrap_size(),
    }
}

#[cfg(target_os = "linux")]
fn open_transcript_window(cx: &mut App) {
    let options = WindowOptions {
        window_bounds: Some(WindowBounds::Windowed(transcript_window_bounds(cx))),
        app_id: Some(CLIENT_APP_ID.to_string()),
        focus: false,
        kind: WindowKind::Normal,
        is_movable: true,
        is_resizable: true,
        is_minimizable: true,
        window_background: WindowBackgroundAppearance::Opaque,
        ..Default::default()
    };

    let transcript_window = cx
        .open_window(options, |_, cx| {
            let input = cx.new(linux_transcript_input::TranscriptInput::new);
            cx.new(|_| TranscriptWindow {
                input,
                last_voice_text: String::new(),
            })
        })
        .expect("failed to open transcript window");
    transcript_window
        .update(cx, |view, window, cx| {
            window.focus(&view.input.focus_handle(cx));
            window.on_window_should_close(cx, |_, _| {
                reset_transcript_window_state();
                true
            });
        })
        .expect("failed to focus transcript editor");
    cx.on_window_closed(|_| reset_transcript_window_state())
        .detach();
    platform::configure_transcript_window();
    platform::set_transcript_window_visible(true);
}

struct VoiceOverlay;

impl Render for VoiceOverlay {
    fn render(&mut self, window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        window.request_animation_frame();
        let phase = overlay_phase();
        #[cfg(target_os = "linux")]
        {
            platform::sync_overlay_window(window, phase);
            let requested = TRANSCRIPT_WINDOW_REQUESTED.load(Ordering::Acquire);
            let created = TRANSCRIPT_WINDOW_CREATED.load(Ordering::Acquire);
            if should_create_transcript_window(requested, created)
                && TRANSCRIPT_WINDOW_CREATED
                    .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
                    .is_ok()
            {
                _cx.defer(open_transcript_window);
            }
        }

        div()
            .size_full()
            .flex()
            .items_center()
            .justify_center()
            .with_animation(
                "overlay-clock",
                Animation::new(Duration::from_millis(1_120)).repeat(),
                move |root, delta| {
                    let content = match phase {
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
    #[cfg(target_os = "linux")]
    if std::env::args().any(|arg| arg == "--settings") {
        settings_gui::run();
        return;
    }

    #[cfg(target_os = "linux")]
    if linux_gnome_shortcuts::forward_toggle_invocation() {
        return;
    }

    #[cfg(target_os = "linux")]
    platform::prefer_xwayland_overlay();

    #[cfg(target_os = "windows")]
    if !platform::claim_single_instance() {
        return;
    }
    #[cfg(target_os = "linux")]
    if !platform::claim_single_instance() {
        return;
    }

    Application::new().run(|cx: &mut App| {
        #[cfg(target_os = "linux")]
        cx.bind_keys([
            KeyBinding::new(
                "backspace",
                linux_transcript_input::Backspace,
                Some("TranscriptInput"),
            ),
            KeyBinding::new(
                "delete",
                linux_transcript_input::Delete,
                Some("TranscriptInput"),
            ),
            KeyBinding::new(
                "left",
                linux_transcript_input::Left,
                Some("TranscriptInput"),
            ),
            KeyBinding::new(
                "right",
                linux_transcript_input::Right,
                Some("TranscriptInput"),
            ),
            KeyBinding::new(
                "shift-left",
                linux_transcript_input::SelectLeft,
                Some("TranscriptInput"),
            ),
            KeyBinding::new(
                "shift-right",
                linux_transcript_input::SelectRight,
                Some("TranscriptInput"),
            ),
            KeyBinding::new(
                "cmd-a",
                linux_transcript_input::SelectAll,
                Some("TranscriptInput"),
            ),
            KeyBinding::new(
                "cmd-c",
                linux_transcript_input::Copy,
                Some("TranscriptInput"),
            ),
            KeyBinding::new(
                "cmd-x",
                linux_transcript_input::Cut,
                Some("TranscriptInput"),
            ),
            KeyBinding::new(
                "cmd-v",
                linux_transcript_input::Paste,
                Some("TranscriptInput"),
            ),
            KeyBinding::new(
                "home",
                linux_transcript_input::Home,
                Some("TranscriptInput"),
            ),
            KeyBinding::new("end", linux_transcript_input::End, Some("TranscriptInput")),
        ]);

        let options = WindowOptions {
            window_bounds: Some(WindowBounds::Windowed(overlay_bounds(cx))),
            titlebar: None,
            app_id: cfg!(target_os = "linux").then(|| CLIENT_APP_ID.to_string()),
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
    use std::time::Duration;

    use super::native_voice::{NativeVoiceConfig, NativeVoiceController};
    use super::platform_paste::PasteTarget;
    use super::{
        LISTENING_CAPSULE_HEIGHT, LISTENING_CAPSULE_WIDTH, NativeVoiceEventOutcome, OVERLAY_HEIGHT,
        OVERLAY_WIDTH, OverlayPhase, apply_native_voice_event, clear_voice_activity,
        dark_background, decide_dark_background, next_overlay_generation, overlay_generation,
        overlay_phase, set_dark_background, set_overlay_phase,
    };
    use gpui::Window;
    use raw_window_handle::{HasWindowHandle, RawWindowHandle};
    use windows::Win32::Foundation::{
        ERROR_ALREADY_EXISTS, GetLastError, HWND, LPARAM, RECT, WPARAM,
    };
    use windows::Win32::Graphics::Dwm::{
        DWMNCRP_DISABLED, DWMWA_BORDER_COLOR, DWMWA_COLOR_NONE, DWMWA_NCRENDERING_POLICY,
        DwmSetWindowAttribute,
    };
    use windows::Win32::Graphics::Gdi::{
        CreateRoundRectRgn, GetDC, GetMonitorInfoW, GetPixel, MONITOR_DEFAULTTOPRIMARY,
        MONITORINFO, MonitorFromWindow, ReleaseDC, SetWindowRgn,
    };
    use windows::Win32::System::Threading::CreateMutexW;
    use windows::Win32::UI::Input::KeyboardAndMouse::{
        MOD_ALT, MOD_CONTROL, RegisterHotKey, VK_SPACE,
    };
    use windows::Win32::UI::WindowsAndMessaging::{
        FindWindowW, GWL_EXSTYLE, GWL_STYLE, GetMessageW, GetWindowLongPtrW, GetWindowRect,
        HWND_TOPMOST, IsWindowVisible, MB_ICONERROR, MB_OK, MSG, MessageBoxW, SW_HIDE, SW_SHOW,
        SW_SHOWNOACTIVATE, SWP_FRAMECHANGED, SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOSIZE,
        SetForegroundWindow, SetWindowLongPtrW, SetWindowPos, ShowWindow, WM_HOTKEY, WS_BORDER,
        WS_DLGFRAME, WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW, WS_THICKFRAME,
    };
    use windows::core::w;

    const HOTKEY_ID: i32 = 0xDB01;
    const OPTIMIZING_DURATION: Duration = Duration::from_millis(2_400);
    /// How far outside the capsule the background probes sit. The overlay window is
    /// clipped to the capsule by `clip_overlay_to_capsule`, so pixels this far out are
    /// desktop rather than glass -- which is what lets the probes run while the capsule
    /// is on screen instead of only before it appears.
    const BACKGROUND_PROBE_MARGIN: i32 = 6;
    /// Re-check often enough that dragging a window under the capsule feels immediate.
    const BACKGROUND_SAMPLE_INTERVAL: Duration = Duration::from_millis(150);
    /// The capsule is hidden for almost the whole life of the process, so the sampler
    /// idles at a much longer period rather than waking seven times a second forever.
    const BACKGROUND_IDLE_INTERVAL: Duration = Duration::from_millis(1_000);
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
        hide_overlay(hwnd);
        start_hotkey_thread(hwnd.0 as isize);
        start_background_sampler_thread(hwnd.0 as isize);
        super::windows_shell::start(hwnd);
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
        }
    }

    fn begin_input(hwnd: HWND) {
        let generation = next_overlay_generation();
        clear_voice_activity();
        set_dark_background(sample_dark_background(hwnd, None));
        show_phase(hwnd, OverlayPhase::Activating);
        if let Err(error) = start_voice_client(generation, hwnd) {
            append_voice_client_log(&format!("[gpui] failed to start voice client: {error}\n"));
            hide_overlay(hwnd);
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
        VOICE_CLIENT.start(config, move |event| {
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
    }

    fn stop_voice_client() -> bool {
        VOICE_CLIENT.request_stop()
    }

    /// Reads the desktop in a ring around the capsule and decides which palette the
    /// glass should wear.
    ///
    /// The probes sit outside the capsule on purpose. Sampling inside it only works
    /// before the overlay is shown -- afterwards the probes would land on the glass and
    /// the reading would feed back on itself. A ring keeps the answer meaningful at any
    /// time, which is what allows the palette to follow a window dragged underneath.
    ///
    /// `previous` is the palette currently on screen, or `None` for a first look, which
    /// is judged on a single threshold rather than the hysteresis band.
    fn sample_dark_background(hwnd: HWND, previous: Option<bool>) -> bool {
        let fallback = previous.unwrap_or(false);
        let Some(capsule) = capsule_screen_rect(hwnd) else {
            return fallback;
        };

        let width = capsule.right - capsule.left;
        let height = capsule.bottom - capsule.top;
        let margin = height.max(3) / 3 + BACKGROUND_PROBE_MARGIN;

        // Five probes along the top and bottom edges, two down each side. Points that
        // fall off the desktop come back as CLR_INVALID and are skipped below, so a
        // capsule near a screen edge simply votes with fewer probes.
        let mut probes = Vec::with_capacity(14);
        for step in 1..=5 {
            let x = capsule.left + width * step / 6;
            probes.push((x, capsule.top - margin));
            probes.push((x, capsule.bottom + margin));
        }
        for step in 1..=2 {
            let y = capsule.top + height * step / 3;
            probes.push((capsule.left - margin, y));
            probes.push((capsule.right + margin, y));
        }

        unsafe {
            let screen = GetDC(None);
            if screen.is_invalid() {
                return fallback;
            }

            let mut luminance_sum = 0u32;
            let mut dark_samples = 0u32;
            let mut valid_samples = 0u32;

            for (x, y) in probes {
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

            let _ = ReleaseDC(None, screen);
            if valid_samples == 0 {
                return fallback;
            }

            decide_dark_background(
                luminance_sum / valid_samples,
                dark_samples,
                valid_samples,
                previous,
            )
        }
    }

    /// Keeps the palette in step with whatever ends up behind the capsule while it is
    /// on screen. The one-shot sample in `begin_input` only sees the desktop at the
    /// instant the overlay opens; without this the glass keeps that opening palette even
    /// after a window is dragged underneath it.
    fn start_background_sampler_thread(hwnd_value: isize) {
        thread::spawn(move || {
            let hwnd = HWND(hwnd_value as *mut c_void);
            loop {
                let visible = unsafe { IsWindowVisible(hwnd) }.as_bool();
                if visible {
                    set_dark_background(sample_dark_background(hwnd, Some(dark_background())));
                }
                thread::sleep(if visible {
                    BACKGROUND_SAMPLE_INTERVAL
                } else {
                    BACKGROUND_IDLE_INTERVAL
                });
            }
        });
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
            if RegisterHotKey(None, HOTKEY_ID, MOD_CONTROL | MOD_ALT, VK_SPACE.0 as u32).is_err() {
                append_voice_client_log(
                    "[gpui] could not register Ctrl+Alt+Space; another application may own it\n",
                );
                let _ = MessageBoxW(
                    None,
                    w!(
                        "Could not register Ctrl+Alt+Space. Another application may already use \
                         this shortcut. Close the conflicting application, then restart Doubao \
                         Voice Client."
                    ),
                    w!("Doubao Voice Client"),
                    MB_OK | MB_ICONERROR,
                );
                return;
            }

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
}

#[cfg(test)]
mod tests {
    use super::{
        BAR_COUNT, OverlayPhase, VoiceHotkeyAction, VoiceTranscript, decoded_voice_activity,
        is_partial_speech_line, is_strong_voice_activity_line, overlay_phase_from_bridge_line,
        transcript_presentation, voice_activity_from_audio_line, voice_hotkey_action,
        waveform_bar_height,
    };
    #[cfg(target_os = "windows")]
    use super::decide_dark_background;

    /// A ring where only a couple of probes read dark, so the majority rule stays out of
    /// the way and the luminance thresholds are what actually decide.
    #[cfg(target_os = "windows")]
    const MIXED_RING: (u32, u32) = (2, 14);

    #[cfg(target_os = "windows")]
    #[test]
    fn a_first_look_uses_one_threshold() {
        let (dark, valid) = MIXED_RING;
        assert!(decide_dark_background(140, dark, valid, None));
        assert!(!decide_dark_background(160, dark, valid, None));
    }

    #[cfg(target_os = "windows")]
    #[test]
    fn hysteresis_holds_the_palette_through_a_borderline_background() {
        let (dark, valid) = MIXED_RING;
        // 140 would read as dark on a first look, but is not dark enough to pull an
        // already-light capsule across.
        assert!(!decide_dark_background(140, dark, valid, Some(false)));
        // 160 would read as light on a first look, yet leaves a dark capsule dark.
        assert!(decide_dark_background(160, dark, valid, Some(true)));
    }

    #[cfg(target_os = "windows")]
    #[test]
    fn a_clearly_changed_background_still_switches_the_palette() {
        let (dark, valid) = MIXED_RING;
        assert!(decide_dark_background(120, dark, valid, Some(false)));
        assert!(!decide_dark_background(175, dark, valid, Some(true)));
    }

    #[cfg(target_os = "windows")]
    #[test]
    fn a_mostly_dark_ring_wins_over_a_bright_average() {
        assert!(decide_dark_background(200, 7, 14, Some(false)));
    }

    #[cfg(target_os = "windows")]
    #[test]
    fn losing_every_probe_keeps_the_current_palette() {
        assert!(decide_dark_background(0, 0, 0, Some(true)));
        assert!(!decide_dark_background(0, 0, 0, Some(false)));
        assert!(!decide_dark_background(0, 0, 0, None));
    }

    #[test]
    fn transcript_tracks_partial_committed_and_final_text() {
        let mut transcript = VoiceTranscript::default();
        transcript.committed("上一段".to_string());
        transcript.begin_session();
        assert_eq!(transcript.visible_text(), "");

        transcript.partial("正在识别".to_string());
        assert_eq!(transcript.visible_text(), "正在识别");

        transcript.committed("已经识别".to_string());
        assert_eq!(transcript.visible_text(), "已经识别");

        transcript.final_text("最终文本".to_string());
        assert_eq!(transcript.visible_text(), "最终文本");
    }

    #[test]
    fn transcript_presentation_explains_empty_and_live_states() {
        assert_eq!(
            transcript_presentation(OverlayPhase::Listening, ""),
            ("正在聆听", "识别到的文字会显示在这里")
        );
        assert_eq!(
            transcript_presentation(OverlayPhase::Listening, "你好"),
            ("实时转写", "你好")
        );
        assert_eq!(
            transcript_presentation(OverlayPhase::Optimizing, "你好世界"),
            ("识别结果", "你好世界")
        );
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn transcript_window_does_not_resize_the_voice_overlay() {
        assert_eq!((super::OVERLAY_WIDTH, super::OVERLAY_HEIGHT), (128.0, 42.0));
        assert_eq!(
            (
                super::TRANSCRIPT_WINDOW_WIDTH,
                super::TRANSCRIPT_WINDOW_HEIGHT
            ),
            (560.0, 240.0)
        );
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn transcript_window_stays_visible_after_a_result() {
        assert!(!super::transcript_window_should_be_visible(
            OverlayPhase::Hidden,
            false,
        ));
        assert!(super::transcript_window_should_be_visible(
            OverlayPhase::Hidden,
            true,
        ));
        assert!(super::transcript_window_should_be_visible(
            OverlayPhase::Activating,
            false,
        ));
        assert!(super::transcript_window_should_be_visible(
            OverlayPhase::Listening,
            false,
        ));
        assert!(super::transcript_window_should_be_visible(
            OverlayPhase::Optimizing,
            false,
        ));
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn closing_transcript_allows_the_next_activation_to_recreate_it() {
        assert_eq!(
            super::transcript_state_after_user_close(true, true),
            (false, false)
        );
        assert!(super::should_create_transcript_window(true, false));
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn transcript_window_is_kept_above_the_active_application() {
        assert_eq!(
            super::transcript_window_state_atoms(),
            ["_NET_WM_STATE_ABOVE"]
        );
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn transcript_editor_sync_preserves_manual_edits_until_voice_text_changes() {
        assert!(!super::should_apply_voice_snapshot("语音原文", "语音原文"));
        assert!(super::should_apply_voice_snapshot("语音原文", "新的识别"));
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn hidden_transcript_geometry_is_offscreen_without_unmapping() {
        let saved = (680, 420, 560, 240);
        assert_eq!(
            super::transcript_window_geometry(false, saved),
            (-100, -100, 1, 1)
        );
        assert_eq!(super::transcript_window_geometry(true, saved), saved);
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn visible_transcript_requests_ewmh_state_add() {
        assert_eq!(super::net_wm_state_action(true), 1);
        assert_eq!(super::net_wm_state_action(false), 0);
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn transcript_window_is_created_lazily_on_first_activation() {
        assert!(!super::should_create_transcript_window(false, false));
        assert!(super::should_create_transcript_window(true, false));
        assert!(!super::should_create_transcript_window(true, true));
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn transcript_window_is_centered_just_above_the_overlay() {
        assert_eq!(
            super::transcript_geometry_above_overlay((898, 1016, 128, 42)),
            (682, 762, 560, 240)
        );
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn transcript_bootstrap_is_invisible_at_its_final_origin() {
        assert_eq!(
            super::transcript_bootstrap_size(),
            gpui::size(gpui::px(1.0), gpui::px(1.0))
        );
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn transcript_position_accounts_for_window_manager_frame() {
        assert_eq!(
            super::transcript_geometry_for_frame((682, 762, 560, 240), (0, 0, 37, 0)),
            (682, 725, 560, 240)
        );
    }

    #[test]
    fn voice_shortcut_starts_only_from_hidden_state() {
        assert_eq!(
            voice_hotkey_action(OverlayPhase::Hidden),
            VoiceHotkeyAction::Begin
        );
        assert_eq!(
            voice_hotkey_action(OverlayPhase::Optimizing),
            VoiceHotkeyAction::Ignore
        );
    }

    #[test]
    fn voice_shortcut_finishes_activation_or_recording() {
        assert_eq!(
            voice_hotkey_action(OverlayPhase::Activating),
            VoiceHotkeyAction::Finish
        );
        assert_eq!(
            voice_hotkey_action(OverlayPhase::Listening),
            VoiceHotkeyAction::Finish
        );
    }

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
            overlay_phase_from_bridge_line("[bridge_phase] ui_ready"),
            Some(OverlayPhase::Activating)
        );
        assert_eq!(
            overlay_phase_from_bridge_line("[bridge_phase] asr_warmup"),
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
    use std::{
        fs::{File, OpenOptions},
        path::PathBuf,
        sync::{
            Mutex, OnceLock,
            atomic::{AtomicU32, Ordering},
        },
        thread,
        time::{Duration, Instant},
    };

    use fs2::FileExt as _;
    use gpui::Window;
    use raw_window_handle::{HasWindowHandle, RawWindowHandle};
    use x11rb::{
        connection::Connection,
        protocol::{
            Event,
            xkb::{BoolCtrl, ConnectionExt as _, ID, PerClientFlag},
            xproto::{
                AtomEnum, ChangeWindowAttributesAux, ClientMessageEvent, ConnectionExt as _,
                EventMask, GrabMode, Keycode, ModMask, StackMode, Window as X11Window,
            },
        },
        rust_connection::RustConnection,
    };
    use xkbcommon::xkb::keysyms;

    use super::linux_shortcut::{LinuxShortcut, ShortcutModifiers};
    use super::linux_tray::TrayPhase;
    use super::native_voice::{NativeVoiceConfig, NativeVoiceController, NativeVoiceEvent};
    use super::{
        NativeVoiceEventOutcome, OVERLAY_HEIGHT, OVERLAY_WIDTH, OverlayPhase,
        TRANSCRIPT_WINDOW_CREATED, TRANSCRIPT_WINDOW_HEIGHT, TRANSCRIPT_WINDOW_REQUESTED,
        TRANSCRIPT_WINDOW_WIDTH, VoiceHotkeyAction, apply_native_voice_event, clear_voice_activity,
        net_wm_state_action, next_overlay_generation, overlay_generation, overlay_phase, px,
        reset_transcript_window_state, set_overlay_phase, size, transcript_geometry_above_overlay,
        transcript_geometry_for_frame, transcript_window_geometry,
        transcript_window_should_be_visible, transcript_window_state_atoms, voice_hotkey_action,
        with_voice_transcript,
    };

    static INSTANCE_LOCK: OnceLock<File> = OnceLock::new();
    static X11_OVERLAY_SIZE: OnceLock<(u32, u32)> = OnceLock::new();
    static X11_OVERLAY_WINDOW: OnceLock<X11Window> = OnceLock::new();
    static TRANSCRIPT_X11_WINDOW: AtomicU32 = AtomicU32::new(0);
    static TRANSCRIPT_X11_GEOMETRY: OnceLock<(i32, i32, u32, u32)> = OnceLock::new();
    static VOICE_ACTION_LOCK: Mutex<()> = Mutex::new(());
    static VOICE_CLIENT: NativeVoiceController = NativeVoiceController::new();

    pub fn prefer_xwayland_overlay() {
        if std::env::var_os("DOUBAO_NATIVE_WAYLAND_OVERLAY").is_some()
            || std::env::var_os("WAYLAND_DISPLAY").is_none()
            || std::env::var_os("DISPLAY").is_none()
        {
            return;
        }

        // GPUI's native Wayland path cannot reliably map and position this
        // no-activate overlay. XWayland gives us an X11 window id that the
        // client can explicitly map/unmap for each voice session.
        unsafe {
            std::env::remove_var("WAYLAND_DISPLAY");
            std::env::set_var("DOUBAO_XWAYLAND_OVERLAY", "1");
        }
        eprintln!("[linux-client] using XWayland for the GPUI overlay");
    }

    #[derive(Clone, Copy)]
    enum LinuxSessionMode {
        OneShot,
        X11(X11Window),
        Portal(Option<X11Window>),
    }

    impl LinuxSessionMode {
        fn x11_window(self) -> Option<X11Window> {
            match self {
                Self::X11(window) | Self::Portal(Some(window)) => Some(window),
                Self::OneShot | Self::Portal(None) => None,
            }
        }
    }

    pub fn claim_single_instance() -> bool {
        let Some(runtime_dir) = std::env::var_os("XDG_RUNTIME_DIR").map(PathBuf::from) else {
            eprintln!("[linux-client] XDG_RUNTIME_DIR is not set; refusing to run without a lock");
            return false;
        };
        let lock_path = runtime_dir.join("doubao-voice-client.lock");
        let file = match OpenOptions::new()
            .create(true)
            .read(true)
            .write(true)
            .truncate(false)
            .open(&lock_path)
        {
            Ok(file) => file,
            Err(error) => {
                eprintln!(
                    "[linux-client] could not open instance lock {}: {error}",
                    lock_path.display()
                );
                return false;
            }
        };
        if let Err(error) = file.try_lock_exclusive() {
            eprintln!("[linux-client] another client instance is already running: {error}");
            return false;
        }
        INSTANCE_LOCK.set(file).is_ok()
    }

    pub fn sync_overlay_window(window: &mut Window, phase: OverlayPhase) {
        if !is_native_wayland_overlay() {
            return;
        }
        let Ok(handle) = HasWindowHandle::window_handle(window) else {
            return;
        };
        if !matches!(handle.as_raw(), RawWindowHandle::Wayland(_)) {
            return;
        }
        let desired_size = if phase == OverlayPhase::Hidden {
            size(px(1.0), px(1.0))
        } else {
            size(px(OVERLAY_WIDTH), px(OVERLAY_HEIGHT))
        };
        if window.bounds().size != desired_size {
            window.resize(desired_size);
        }
    }

    pub fn configure_overlay(_window: &Window) {
        if let Err(error) = ctrlc::set_handler(|| {
            if stop_voice_client() {
                set_overlay_phase(OverlayPhase::Optimizing);
            } else {
                std::process::exit(0);
            }
        }) {
            eprintln!("[linux-client] could not install signal handler: {error}");
        }

        let x_window = discover_x11_window();
        let shortcut = match LinuxShortcut::from_environment() {
            Ok(shortcut) => shortcut,
            Err(error) => {
                eprintln!("[linux-client] {error}; global shortcut disabled");
                let mode = match x_window {
                    Some(x_window) if !is_wayland_session() => LinuxSessionMode::X11(x_window),
                    x_window => LinuxSessionMode::Portal(x_window),
                };
                fall_back_to_tray(mode, false);
                return;
            }
        };

        match x_window {
            Some(x_window) if is_wayland_session() => {
                let mode = LinuxSessionMode::Portal(Some(x_window));
                let tray_available = start_tray(mode);
                start_wayland_shortcut_listener(Some(x_window), shortcut, tray_available);
            }
            Some(x_window) => match register_x11_hotkey(&shortcut) {
                Ok((connection, keycode)) => {
                    clear_voice_activity();
                    set_overlay_phase(OverlayPhase::Hidden);
                    set_x11_window_visible(x_window, false);
                    let tray_available = start_tray(LinuxSessionMode::X11(x_window));
                    start_x11_hotkey_thread(
                        x_window,
                        connection,
                        keycode,
                        shortcut.trigger().to_string(),
                        tray_available,
                    );
                    eprintln!(
                        "[linux-client] ready; press {} to start or finish voice input",
                        shortcut.trigger()
                    );
                }
                Err(error) => {
                    eprintln!(
                        "[linux-client] could not register {} ({error}); \
                         falling back to tray controls",
                        shortcut.trigger()
                    );
                    fall_back_to_tray(
                        LinuxSessionMode::X11(x_window),
                        error.allows_legacy_one_shot(),
                    );
                }
            },
            None => {
                let mode = LinuxSessionMode::Portal(None);
                let tray_available = start_tray(mode);
                start_wayland_shortcut_listener(None, shortcut, tray_available);
            }
        }
    }

    pub fn configure_transcript_window() {
        let Some(overlay_window) = X11_OVERLAY_WINDOW.get().copied() else {
            return;
        };
        let deadline = Instant::now() + Duration::from_millis(500);
        while Instant::now() < deadline {
            match find_x11_window_for_current_process_excluding(overlay_window) {
                Ok(Some(window)) => {
                    if let Ok(geometry) = connection_geometry(window) {
                        let _ = TRANSCRIPT_X11_GEOMETRY.set(geometry);
                    }
                    TRANSCRIPT_X11_WINDOW.store(window, Ordering::Release);
                    return;
                }
                Ok(None) => thread::sleep(Duration::from_millis(20)),
                Err(error) => {
                    eprintln!("[linux-client] could not discover transcript window: {error}");
                    return;
                }
            }
        }
        eprintln!("[linux-client] could not discover transcript window");
    }

    fn fall_back_to_tray(mode: LinuxSessionMode, allow_legacy_one_shot: bool) {
        clear_voice_activity();
        set_overlay_phase(OverlayPhase::Hidden);
        if let Some(x_window) = mode.x11_window() {
            set_x11_window_visible(x_window, false);
        }
        if start_tray(mode) {
            return;
        }
        if allow_legacy_one_shot {
            eprintln!("[linux-client] tray unavailable; starting the legacy one-shot session");
            begin_input(LinuxSessionMode::OneShot);
        } else {
            eprintln!("[linux-client] tray unavailable; exiting without starting the microphone");
            std::process::exit(1);
        }
    }

    fn start_portal_shortcut_listener(
        x_window: Option<X11Window>,
        preferred_trigger: String,
        tray_available: bool,
    ) {
        clear_voice_activity();
        set_overlay_phase(OverlayPhase::Hidden);
        if let Some(x_window) = x_window {
            set_x11_window_visible(x_window, false);
        }
        eprintln!(
            "[linux-client] requesting {preferred_trigger} through XDG Global Shortcuts Portal"
        );
        super::linux_global_shortcuts::start(
            preferred_trigger,
            move || handle_voice_shortcut(LinuxSessionMode::Portal(x_window)),
            move |error| {
                eprintln!("[linux-client] {error}");
                if tray_available {
                    eprintln!("[linux-client] continuing with tray controls");
                    return;
                }
                if overlay_phase() != OverlayPhase::Hidden {
                    set_overlay_phase(OverlayPhase::Optimizing);
                }
                VOICE_CLIENT.stop_and_wait();
                std::process::exit(1);
            },
        );
    }

    fn start_wayland_shortcut_listener(
        x_window: Option<X11Window>,
        shortcut: LinuxShortcut,
        tray_available: bool,
    ) {
        clear_voice_activity();
        set_overlay_phase(OverlayPhase::Hidden);
        if let Some(x_window) = x_window {
            set_x11_window_visible(x_window, false);
        }

        if super::linux_gnome_shortcuts::is_gnome_desktop() {
            let mode = LinuxSessionMode::Portal(x_window);
            match super::linux_gnome_shortcuts::start(&shortcut, move || {
                handle_voice_shortcut(mode)
            }) {
                Ok(()) => {
                    eprintln!(
                        "[linux-client] global shortcut ready through GNOME settings: {}",
                        shortcut.trigger()
                    );
                    return;
                }
                Err(error) => {
                    eprintln!(
                        "[linux-client] GNOME shortcut setup failed ({error}); \
                         trying XDG Global Shortcuts Portal"
                    );
                }
            }
        }

        start_portal_shortcut_listener(x_window, shortcut.trigger().to_string(), tray_available);
    }

    fn start_tray(mode: LinuxSessionMode) -> bool {
        match super::linux_tray::start(
            tray_phase,
            move || handle_voice_shortcut(mode),
            open_settings,
            quit_client,
        ) {
            Ok(()) => {
                eprintln!("[linux-client] system tray ready");
                true
            }
            Err(error) => {
                eprintln!("[linux-client] system tray unavailable: {error}");
                false
            }
        }
    }

    fn tray_phase() -> TrayPhase {
        match overlay_phase() {
            OverlayPhase::Hidden => TrayPhase::Idle,
            OverlayPhase::Activating => TrayPhase::Activating,
            OverlayPhase::Listening => TrayPhase::Listening,
            OverlayPhase::Optimizing => TrayPhase::Finishing,
        }
    }

    fn quit_client() {
        if overlay_phase() != OverlayPhase::Hidden {
            set_overlay_phase(OverlayPhase::Optimizing);
        }
        VOICE_CLIENT.stop_and_wait();
        std::process::exit(0);
    }

    fn open_settings() {
        let Ok(executable) = std::env::current_exe() else {
            eprintln!("[linux-client] could not locate executable for settings window");
            return;
        };
        if let Err(error) = std::process::Command::new(executable)
            .arg("--settings")
            .spawn()
        {
            eprintln!("[linux-client] could not open settings window: {error}");
        }
    }

    fn is_wayland_session() -> bool {
        std::env::var("XDG_SESSION_TYPE")
            .is_ok_and(|session_type| session_type.eq_ignore_ascii_case("wayland"))
    }

    pub fn finish_input(window: &mut Window) {
        if stop_voice_client() {
            set_overlay_phase(OverlayPhase::Optimizing);
        } else if is_native_wayland_overlay() {
            window.remove_window();
        }
    }

    fn begin_input(mode: LinuxSessionMode) {
        clear_voice_activity();
        with_voice_transcript(|transcript| transcript.begin_session());
        set_overlay_phase(OverlayPhase::Activating);
        if TRANSCRIPT_WINDOW_CREATED.load(Ordering::Acquire) && !transcript_window_exists() {
            reset_transcript_window_state();
        }
        TRANSCRIPT_WINDOW_REQUESTED.store(true, Ordering::Release);
        set_transcript_window_visible(true);
        eprintln!(
            "[linux-client] voice input starting; overlay_window={:?}",
            mode.x11_window()
        );
        if let Some(x_window) = mode.x11_window() {
            set_x11_window_visible(x_window, true);
        }
        if let Err(error) = start_voice_client(mode) {
            eprintln!("[linux-client] failed to start voice client: {error}");
            set_overlay_phase(OverlayPhase::Hidden);
            if !TRANSCRIPT_WINDOW_CREATED.load(Ordering::Acquire) {
                TRANSCRIPT_WINDOW_REQUESTED.store(false, Ordering::Release);
            }
            set_transcript_window_visible(false);
            if let Some(x_window) = mode.x11_window() {
                set_x11_window_visible(x_window, false);
            }
        }
    }

    fn start_voice_client(mode: LinuxSessionMode) -> Result<(), String> {
        let generation = next_overlay_generation();
        let config = NativeVoiceConfig::from_environment()?;
        VOICE_CLIENT.start(config, move |event| {
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
                    clear_voice_activity();
                    set_overlay_phase(OverlayPhase::Hidden);
                    set_transcript_window_visible(transcript_window_should_be_visible(
                        OverlayPhase::Hidden,
                        true,
                    ));
                    match mode {
                        LinuxSessionMode::OneShot => std::process::exit(0),
                        LinuxSessionMode::X11(x_window)
                        | LinuxSessionMode::Portal(Some(x_window)) => {
                            set_x11_window_visible(x_window, false);
                        }
                        LinuxSessionMode::Portal(None) => {}
                    }
                }
            }
        })
    }

    fn stop_voice_client() -> bool {
        VOICE_CLIENT.request_stop()
    }

    fn handle_voice_shortcut(mode: LinuxSessionMode) {
        let Ok(_action) = VOICE_ACTION_LOCK.lock() else {
            eprintln!("[linux-client] voice action lock is poisoned");
            return;
        };
        match voice_hotkey_action(overlay_phase()) {
            VoiceHotkeyAction::Begin => begin_input(mode),
            VoiceHotkeyAction::Finish => {
                if stop_voice_client() {
                    set_overlay_phase(OverlayPhase::Optimizing);
                }
            }
            VoiceHotkeyAction::Ignore => {}
        }
    }

    fn discover_x11_window() -> Option<X11Window> {
        if std::env::var_os("DISPLAY").is_none() {
            return None;
        }
        if is_native_wayland_overlay() {
            return None;
        }

        let deadline = Instant::now() + Duration::from_millis(500);
        while Instant::now() < deadline {
            match find_x11_window_for_current_process() {
                Ok(Some(window)) => {
                    remember_x11_overlay_size(window);
                    let _ = X11_OVERLAY_WINDOW.set(window);
                    return Some(window);
                }
                Ok(None) => thread::sleep(Duration::from_millis(20)),
                Err(error) => {
                    eprintln!("[linux-client] could not discover GPUI X11 overlay window: {error}");
                    return None;
                }
            }
        }
        eprintln!(
            "[linux-client] could not find GPUI X11 overlay window; overlay controls disabled"
        );
        None
    }

    fn find_x11_window_for_current_process() -> Result<Option<X11Window>, String> {
        let (connection, screen_num) =
            RustConnection::connect(None).map_err(|error| error.to_string())?;
        let pid_atom = connection
            .intern_atom(false, b"_NET_WM_PID")
            .map_err(|error| error.to_string())?
            .reply()
            .map_err(|error| error.to_string())?
            .atom;
        let root = connection.setup().roots[screen_num].root;
        let tree = connection
            .query_tree(root)
            .map_err(|error| error.to_string())?
            .reply()
            .map_err(|error| error.to_string())?;
        let current_pid = std::process::id();

        for window in tree.children.iter().rev().copied() {
            let Ok(reply) = connection
                .get_property(false, window, pid_atom, AtomEnum::CARDINAL, 0, 1)
                .map_err(|error| error.to_string())?
                .reply()
            else {
                continue;
            };
            if reply.value32().and_then(|mut values| values.next()) == Some(current_pid) {
                return Ok(Some(window));
            }
        }

        Ok(None)
    }

    fn find_x11_window_for_current_process_excluding(
        excluded: X11Window,
    ) -> Result<Option<X11Window>, String> {
        let (connection, screen_num) =
            RustConnection::connect(None).map_err(|error| error.to_string())?;
        let pid_atom = connection
            .intern_atom(false, b"_NET_WM_PID")
            .map_err(|error| error.to_string())?
            .reply()
            .map_err(|error| error.to_string())?
            .atom;
        let root = connection.setup().roots[screen_num].root;
        let current_pid = std::process::id();
        let mut pending = vec![root];

        while let Some(parent) = pending.pop() {
            let tree = connection
                .query_tree(parent)
                .map_err(|error| error.to_string())?
                .reply()
                .map_err(|error| error.to_string())?;
            for window in tree.children.into_iter().rev() {
                if window != excluded {
                    let property = connection
                        .get_property(false, window, pid_atom, AtomEnum::CARDINAL, 0, 1)
                        .map_err(|error| error.to_string())?
                        .reply()
                        .map_err(|error| error.to_string())?;
                    if property.value32().and_then(|mut values| values.next()) == Some(current_pid)
                    {
                        return Ok(Some(window));
                    }
                }
                pending.push(window);
            }
        }
        Ok(None)
    }

    fn remember_x11_overlay_size(window: X11Window) {
        if X11_OVERLAY_SIZE.get().is_some() {
            return;
        }
        let Ok((connection, _)) = RustConnection::connect(None) else {
            return;
        };
        let Ok(cookie) = connection.get_geometry(window) else {
            return;
        };
        let Ok(geometry) = cookie.reply() else {
            return;
        };
        let width = u32::from(geometry.width).max(1);
        let height = u32::from(geometry.height).max(1);
        let _ = X11_OVERLAY_SIZE.set((width, height));
    }

    fn is_native_wayland_overlay() -> bool {
        std::env::var_os("WAYLAND_DISPLAY").is_some()
            && std::env::var_os("DOUBAO_XWAYLAND_OVERLAY").is_none()
    }

    fn set_x11_window_visible(window: X11Window, visible: bool) {
        let Ok((connection, _)) = RustConnection::connect(None) else {
            eprintln!("[linux-client] could not connect to X11 to update the overlay");
            return;
        };
        let (width, height) = if visible {
            X11_OVERLAY_SIZE
                .get()
                .copied()
                .unwrap_or((OVERLAY_WIDTH.round() as u32, OVERLAY_HEIGHT.round() as u32))
        } else {
            (1, 1)
        };
        let configure = x11rb::protocol::xproto::ConfigureWindowAux::new()
            .width(width)
            .height(height)
            .stack_mode(StackMode::ABOVE);
        let result = connection
            .configure_window(window, &configure)
            .and_then(|_| connection.map_window(window));
        if result.is_err() || connection.flush().is_err() {
            eprintln!("[linux-client] could not update the X11 overlay visibility");
        } else {
            eprintln!("[linux-client] overlay visible={visible} size={width}x{height}");
        }
    }

    pub fn set_transcript_window_visible(visible: bool) {
        let window = TRANSCRIPT_X11_WINDOW.load(Ordering::Acquire);
        if window == 0 {
            return;
        }
        let Ok((connection, _)) = RustConnection::connect(None) else {
            eprintln!("[linux-client] could not connect to X11 to update the transcript window");
            return;
        };
        let fallback = TRANSCRIPT_X11_GEOMETRY.get().copied().unwrap_or((
            0,
            0,
            TRANSCRIPT_WINDOW_WIDTH.round() as u32,
            TRANSCRIPT_WINDOW_HEIGHT.round() as u32,
        ));
        let aligned = X11_OVERLAY_WINDOW
            .get()
            .copied()
            .and_then(|overlay| connection_geometry(overlay).ok())
            .map(transcript_geometry_above_overlay)
            .unwrap_or(fallback);
        let aligned = transcript_geometry_for_frame(
            aligned,
            window_frame_extents(&connection, window).unwrap_or_default(),
        );
        let (x, y, width, height) = transcript_window_geometry(visible, aligned);
        let state_result = apply_transcript_window_state(&connection, window, visible);
        let result = state_result.and_then(|_| {
            connection
                .configure_window(
                    window,
                    &x11rb::protocol::xproto::ConfigureWindowAux::new()
                        .x(x)
                        .y(y)
                        .width(width)
                        .height(height)
                        .stack_mode(StackMode::ABOVE),
                )
                .map(|_| ())
                .map_err(|error| error.to_string())
        });
        if result.is_err() || connection.flush().is_err() {
            eprintln!("[linux-client] could not update transcript window visibility");
        } else {
            eprintln!("[linux-client] transcript visible={visible}");
        }
    }

    pub fn forget_transcript_window() {
        TRANSCRIPT_X11_WINDOW.store(0, Ordering::Release);
    }

    pub fn transcript_window_exists() -> bool {
        let window = TRANSCRIPT_X11_WINDOW.load(Ordering::Acquire);
        if window == 0 {
            return false;
        }
        let Ok((connection, _)) = RustConnection::connect(None) else {
            return false;
        };
        connection
            .get_window_attributes(window)
            .ok()
            .and_then(|cookie| cookie.reply().ok())
            .is_some()
    }

    fn connection_geometry(window: X11Window) -> Result<(i32, i32, u32, u32), String> {
        let (connection, _) = RustConnection::connect(None).map_err(|error| error.to_string())?;
        let geometry = connection
            .get_geometry(window)
            .map_err(|error| error.to_string())?
            .reply()
            .map_err(|error| error.to_string())?;
        Ok((
            i32::from(geometry.x),
            i32::from(geometry.y),
            u32::from(geometry.width),
            u32::from(geometry.height),
        ))
    }

    fn window_frame_extents(
        connection: &RustConnection,
        window: X11Window,
    ) -> Result<(u32, u32, u32, u32), String> {
        let atom = connection
            .intern_atom(false, b"_NET_FRAME_EXTENTS")
            .map_err(|error| error.to_string())?
            .reply()
            .map_err(|error| error.to_string())?
            .atom;
        let property = connection
            .get_property(false, window, atom, AtomEnum::CARDINAL, 0, 4)
            .map_err(|error| error.to_string())?
            .reply()
            .map_err(|error| error.to_string())?;
        let values = property
            .value32()
            .ok_or_else(|| "_NET_FRAME_EXTENTS is not CARDINAL".to_string())?
            .collect::<Vec<_>>();
        if values.len() < 4 {
            return Err("_NET_FRAME_EXTENTS is incomplete".to_string());
        }
        Ok((values[0], values[1], values[2], values[3]))
    }

    fn apply_transcript_window_state(
        connection: &RustConnection,
        window: X11Window,
        visible: bool,
    ) -> Result<(), String> {
        let state_property = connection
            .intern_atom(false, b"_NET_WM_STATE")
            .map_err(|error| error.to_string())?
            .reply()
            .map_err(|error| error.to_string())?
            .atom;
        let above = connection
            .intern_atom(false, transcript_window_state_atoms()[0].as_bytes())
            .map_err(|error| error.to_string())?
            .reply()
            .map_err(|error| error.to_string())?
            .atom;
        let root = connection.setup().roots[0].root;
        let event = ClientMessageEvent::new(
            32,
            window,
            state_property,
            [net_wm_state_action(visible), above, 0, 1, 0],
        );
        connection
            .send_event(
                false,
                root,
                EventMask::SUBSTRUCTURE_REDIRECT | EventMask::SUBSTRUCTURE_NOTIFY,
                event,
            )
            .map(|_| ())
            .map_err(|error| error.to_string())
    }

    fn start_x11_hotkey_thread(
        x_window: X11Window,
        connection: RustConnection,
        keycode: Keycode,
        trigger: String,
        tray_available: bool,
    ) {
        thread::spawn(move || {
            if let Err(error) = listen_for_x11_hotkey(x_window, connection, keycode) {
                eprintln!("[linux-client] {trigger} listener stopped: {error}");
                if tray_available {
                    eprintln!("[linux-client] continuing with tray controls");
                    return;
                }
                if overlay_phase() != OverlayPhase::Hidden {
                    set_overlay_phase(OverlayPhase::Optimizing);
                }
                VOICE_CLIENT.stop_and_wait();
                std::process::exit(1);
            }
        });
    }

    enum X11HotkeyError {
        Configuration(String),
        Operational(String),
    }

    impl X11HotkeyError {
        fn configuration(error: impl Into<String>) -> Self {
            Self::Configuration(error.into())
        }

        fn operational(error: impl ToString) -> Self {
            Self::Operational(error.to_string())
        }

        fn allows_legacy_one_shot(&self) -> bool {
            matches!(self, Self::Operational(_))
        }
    }

    impl std::fmt::Display for X11HotkeyError {
        fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            match self {
                Self::Configuration(error) | Self::Operational(error) => formatter.write_str(error),
            }
        }
    }

    fn register_x11_hotkey(
        shortcut: &LinuxShortcut,
    ) -> Result<(RustConnection, Keycode), X11HotkeyError> {
        let (connection, screen_number) =
            RustConnection::connect(None).map_err(X11HotkeyError::operational)?;
        let root = connection.setup().roots[screen_number].root;
        let keysym = shortcut.keysym().map_err(X11HotkeyError::configuration)?;
        let keymap = X11Keymap::load(&connection).map_err(X11HotkeyError::operational)?;
        let keycode = keymap
            .keycode_for_primary_group_base_keysym(keysym)
            .ok_or_else(|| {
                X11HotkeyError::configuration(
                    match keymap.keycode_for_keysym_at_any_level(keysym) {
                        Some(_) => format!(
                    "{} is not on the primary X11 keymap group's base layer; use the base key name and add SHIFT if needed",
                    shortcut.key_name()
                ),
                        None => format!(
                            "the primary X11 keymap group does not contain {}",
                            shortcut.key_name()
                        ),
                    },
                )
            })?;
        let modifier_map = connection
            .get_modifier_mapping()
            .map_err(X11HotkeyError::operational)?
            .reply()
            .map_err(X11HotkeyError::operational)?;
        let modifiers = shortcut.modifiers();
        let alt_mask = modifier_mask_for_keysyms(
            &modifier_map.keycodes,
            &keymap,
            &[keysyms::KEY_Alt_L, keysyms::KEY_Alt_R],
        );
        let num_mask =
            modifier_mask_for_keysyms(&modifier_map.keycodes, &keymap, &[keysyms::KEY_Num_Lock]);
        let logo_mask = modifier_mask_for_keysyms(
            &modifier_map.keycodes,
            &keymap,
            &[keysyms::KEY_Super_L, keysyms::KEY_Super_R],
        );
        let required_mask = required_modifier_mask(modifiers, alt_mask, num_mask, logo_mask)
            .map_err(X11HotkeyError::configuration)?;
        enable_detectable_auto_repeat(&connection).map_err(X11HotkeyError::operational)?;

        connection
            .change_window_attributes(
                root,
                &ChangeWindowAttributesAux::new()
                    .event_mask(EventMask::KEY_PRESS | EventMask::KEY_RELEASE),
            )
            .map_err(X11HotkeyError::operational)?
            .check()
            .map_err(X11HotkeyError::operational)?;
        for mask in grab_masks(
            required_mask,
            (!modifiers.num).then_some(num_mask).flatten(),
        ) {
            connection
                .grab_key(false, root, mask, keycode, GrabMode::ASYNC, GrabMode::ASYNC)
                .map_err(X11HotkeyError::operational)?
                .check()
                .map_err(|error| {
                    X11HotkeyError::operational(format!(
                        "could not register {}; it may already be in use: {error}",
                        shortcut.trigger()
                    ))
                })?;
        }
        connection.flush().map_err(X11HotkeyError::operational)?;
        Ok((connection, keycode))
    }

    fn required_modifier_mask(
        modifiers: ShortcutModifiers,
        alt_mask: Option<ModMask>,
        num_mask: Option<ModMask>,
        logo_mask: Option<ModMask>,
    ) -> Result<ModMask, String> {
        let mut mask = ModMask::default();
        if modifiers.control {
            mask |= ModMask::CONTROL;
        }
        if modifiers.shift {
            mask |= ModMask::SHIFT;
        }
        if modifiers.alt {
            mask |= alt_mask.ok_or_else(|| {
                "the active X11 keymap does not define an Alt modifier".to_string()
            })?;
        }
        if modifiers.num {
            mask |= num_mask.ok_or_else(|| {
                "the active X11 keymap does not define a Num Lock modifier".to_string()
            })?;
        }
        if modifiers.logo {
            mask |= logo_mask.ok_or_else(|| {
                "the active X11 keymap does not define a Logo/Super modifier".to_string()
            })?;
        }
        Ok(mask)
    }

    fn modifier_mask_for_keysyms(
        modifier_keycodes: &[Keycode],
        keymap: &X11Keymap,
        target_keysyms: &[u32],
    ) -> Option<ModMask> {
        let keycodes_per_modifier = modifier_keycodes.len().checked_div(8)?;
        if keycodes_per_modifier == 0 {
            return None;
        }
        modifier_keycodes
            .chunks(keycodes_per_modifier)
            .position(|keycodes| {
                keycodes.iter().any(|keycode| {
                    *keycode != 0 && keymap.keycode_contains_any(*keycode, target_keysyms)
                })
            })
            .map(|index| ModMask::from(1_u16 << index))
    }

    fn grab_masks(required: ModMask, num_lock: Option<ModMask>) -> Vec<ModMask> {
        let mut masks = vec![required, required | ModMask::LOCK];
        if let Some(num_lock) = num_lock {
            masks.push(required | num_lock);
            masks.push(required | num_lock | ModMask::LOCK);
        }
        masks.sort_by_key(|mask| u16::from(*mask));
        masks.dedup();
        masks
    }

    fn enable_detectable_auto_repeat(connection: &RustConnection) -> Result<(), String> {
        let extension = connection
            .xkb_use_extension(1, 0)
            .map_err(|error| error.to_string())?
            .reply()
            .map_err(|error| error.to_string())?;
        if !extension.supported {
            return Err("the XKB extension is unavailable".to_string());
        }

        let detectable = PerClientFlag::DETECTABLE_AUTO_REPEAT;
        let flags = connection
            .xkb_per_client_flags(
                ID::USE_CORE_KBD.into(),
                detectable,
                detectable,
                BoolCtrl::default(),
                BoolCtrl::default(),
                BoolCtrl::default(),
            )
            .map_err(|error| error.to_string())?
            .reply()
            .map_err(|error| error.to_string())?;
        if u32::from(flags.value & detectable) == 0 {
            return Err("the X server does not support detectable key repeat".to_string());
        }
        Ok(())
    }

    fn listen_for_x11_hotkey(
        x_window: X11Window,
        connection: RustConnection,
        keycode: Keycode,
    ) -> Result<(), String> {
        let mut shortcut_down = false;
        loop {
            let event = connection
                .wait_for_event()
                .map_err(|error| error.to_string())?;
            match event {
                Event::KeyPress(event) if event.detail == keycode && !shortcut_down => {
                    shortcut_down = true;
                    handle_voice_shortcut(LinuxSessionMode::X11(x_window));
                }
                Event::KeyRelease(event) if event.detail == keycode => shortcut_down = false,
                Event::MappingNotify(_) => {
                    return Err(
                        "the keyboard mapping changed; restart the client to re-register the shortcut"
                            .to_string(),
                    );
                }
                _ => {}
            }
        }
    }

    struct X11Keymap {
        min_keycode: Keycode,
        keysyms_per_keycode: usize,
        keysyms: Vec<u32>,
    }

    impl X11Keymap {
        fn load(connection: &RustConnection) -> Result<Self, String> {
            let setup = connection.setup();
            let min_keycode = setup.min_keycode;
            let keycode_count =
                u8::try_from(u16::from(setup.max_keycode) - u16::from(min_keycode) + 1)
                    .map_err(|_| "the X11 keycode range is too large".to_string())?;
            let mapping = connection
                .get_keyboard_mapping(min_keycode, keycode_count)
                .map_err(|error| error.to_string())?
                .reply()
                .map_err(|error| error.to_string())?;
            let keysyms_per_keycode = usize::from(mapping.keysyms_per_keycode);
            if keysyms_per_keycode == 0 {
                return Err("the X11 keyboard mapping did not contain any keysyms".to_string());
            }
            Ok(Self {
                min_keycode,
                keysyms_per_keycode,
                keysyms: mapping.keysyms,
            })
        }

        fn keycode_for_primary_group_base_keysym(&self, target_keysym: u32) -> Option<Keycode> {
            self.keysyms
                .chunks(self.keysyms_per_keycode)
                .position(|keysyms| keysyms.first() == Some(&target_keysym))
                .map(|offset| self.min_keycode + offset as u8)
        }

        fn keycode_for_keysym_at_any_level(&self, target_keysym: u32) -> Option<Keycode> {
            self.keysyms
                .chunks(self.keysyms_per_keycode)
                .position(|keysyms| keysyms.contains(&target_keysym))
                .map(|offset| self.min_keycode + offset as u8)
        }

        fn keycode_contains_any(&self, keycode: Keycode, target_keysyms: &[u32]) -> bool {
            let Some(offset) = keycode.checked_sub(self.min_keycode).map(usize::from) else {
                return false;
            };
            let start = offset.saturating_mul(self.keysyms_per_keycode);
            self.keysyms
                .get(start..start + self.keysyms_per_keycode)
                .is_some_and(|keysyms| keysyms.iter().any(|keysym| target_keysyms.contains(keysym)))
        }
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
