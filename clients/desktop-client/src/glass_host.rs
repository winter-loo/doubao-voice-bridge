//! A native backdrop VISUAL, not an Acrylic accent attached to the whole HWND.
//!
//! GPUI 0.2.2 owns the HWND's top DirectComposition target. Product construction
//! keeps the lower target and clips its own brush in the composition tree.
//! Explicit visual probes are for the bare HWND example, never the GPUI HWND.
//! No desktop capture/readback or another process's window is involved.

use std::{cell::RefCell, ffi::c_void};
#[path = "glass_host_probe.rs"]
mod probe;
use windows::{
    System::{DispatcherQueue, DispatcherQueueController},
    UI::Composition::{
        CompositionRoundedRectangleGeometry, Compositor, Desktop::DesktopWindowTarget,
        SpriteVisual,
    },
    Win32::{
        Foundation::{HWND, RECT},
        Graphics::Dwm::{DWMWA_USE_HOSTBACKDROPBRUSH, DwmSetWindowAttribute},
        System::WinRT::{
            Composition::ICompositorDesktopInterop, CreateDispatcherQueueController,
            DQTAT_COM_NONE, DQTYPE_THREAD_CURRENT, DispatcherQueueOptions,
        },
        UI::WindowsAndMessaging::GetClientRect,
    },
    core::{BOOL, Interface},
};

fn checked<T>(result: windows::core::Result<T>, operation: &str) -> Result<T, String> {
    result.map_err(|error| format!("{operation}: {error}"))
}

thread_local! {
    // At most one queue is created on this GPUI UI thread. Do not shut the queue
    // down on a theme/policy switch: other composition work can still be pending.
    // If GPUI/the host already provided a queue, we neither replace nor own it.
    static OWNED_QUEUE: RefCell<Option<DispatcherQueueController>> = const { RefCell::new(None) };
}

fn ensure_dispatcher() -> Result<(), String> {
    if DispatcherQueue::GetForCurrentThread().is_ok() {
        return Ok(());
    }
    OWNED_QUEUE.with(|slot| {
        let mut slot = slot.borrow_mut();
        if slot.is_none() {
            let options = DispatcherQueueOptions {
                dwSize: std::mem::size_of::<DispatcherQueueOptions>() as u32,
                threadType: DQTYPE_THREAD_CURRENT,
                // GPUI has already initialized the UI thread's COM apartment.
                apartmentType: DQTAT_COM_NONE,
            };
            *slot = Some(checked(
                unsafe { CreateDispatcherQueueController(options) },
                "create composition dispatcher queue",
            )?);
        }
        Ok(())
    })
}

struct HostOptIn(HWND);
impl HostOptIn {
    fn new(hwnd: HWND) -> Result<Self, String> {
        let enabled = BOOL(1);
        checked(unsafe {
            DwmSetWindowAttribute(
                hwnd, DWMWA_USE_HOSTBACKDROPBRUSH,
                (&enabled as *const BOOL).cast(), std::mem::size_of::<BOOL>() as u32,
            )
        }, "enable native host backdrop (Windows 11 required)")?;
        Ok(Self(hwnd))
    }
}
impl Drop for HostOptIn {
    fn drop(&mut self) {
        let disabled = BOOL(0);
        // The overlay owns this opt-in. Never modify global Windows settings.
        let _ = unsafe {
            DwmSetWindowAttribute(
                self.0, DWMWA_USE_HOSTBACKDROPBRUSH,
                (&disabled as *const BOOL).cast(), std::mem::size_of::<BOOL>() as u32,
            )
        };
    }
}

pub struct HostBackdrop {
    identity: isize,
    target: DesktopWindowTarget,
    compositor: Compositor,
    visual: SpriteVisual,
    geometry: CompositionRoundedRectangleGeometry,
    _opt_in: HostOptIn,
    probe: Option<probe::Probe>,
}

impl HostBackdrop {
    pub fn new(identity: isize, width: u32, height: u32) -> Result<Self, String> {
        // Product construction is fixed: lower slot, untouched default opacity.
        // No command-line or environment option changes these product settings.
        Self::create(identity, width, height, false, None)
    }

    /// Explicit reduction-only entry point. The caller must own a bare HWND
    /// whose target slot is unoccupied; this does not replace an existing target.
    /// Only GlassNativeMinimal calls it. No recovery policy is introduced.
    #[allow(dead_code)]
    pub(crate) fn new_for_target_probe(
        identity: isize, width: u32, height: u32, upper: bool,
    ) -> Result<Self, String> {
        let surface = Self::create(identity, width, height, upper, None)?;
        surface.verify_target(upper)?;
        Ok(surface)
    }

    /// Diagnostic creation-time alpha comparison, always in the lower slot.
    /// No fade, periodic update, source repaint, or post-show reset is performed.
    /// Reduced opacity leaks unfiltered background and can only be evaluated as
    /// a diagnostic until effective blur and foreground legibility are rechecked.
    #[allow(dead_code)]
    pub(crate) fn new_for_opacity_probe(
        identity: isize, width: u32, height: u32, opacity: f32,
    ) -> Result<Self, String> {
        if !opacity.is_finite() || !(0.5..=1.0).contains(&opacity) {
            return Err("diagnostic opacity must be finite and in 0.5..=1.0".into());
        }
        let surface = Self::create(identity, width, height, false, Some(opacity))?;
        surface.verify_target(false)?;
        let actual = checked(surface.visual.Opacity(), "read diagnostic visual opacity")?;
        if !actual.is_finite() || (actual - opacity).abs() > 0.00001 {
            return Err("diagnostic opacity readback mismatch".into());
        }
        eprintln!("[glass-host-opacity] requested={opacity:.4}; actual={actual:.4}; readback=verified; set-before-root=true");
        Ok(surface)
    }

    fn verify_target(&self, upper: bool) -> Result<(), String> {
        let actual = checked(self.target.IsTopmost(), "read created target layer")?;
        if actual != upper {
            return Err("created composition target does not match requested layer".into());
        }
        let requested = if upper { "upper" } else { "lower" };
        let observed = if actual { "upper" } else { "lower" };
        eprintln!("[glass-host-target] requested={requested}; actual={observed}; readback=verified");
        Ok(())
    }

    fn create(identity: isize, width: u32, height: u32, upper: bool, opacity: Option<f32>) -> Result<Self, String> {
        if std::env::var_os("GPUI_DISABLE_DIRECT_COMPOSITION").is_some() {
            return Err("host backdrop requires GPUI's alpha-preserving composition path".into());
        }
        let hwnd = HWND(identity as *mut c_void);
        let opt_in = HostOptIn::new(hwnd)?;
        ensure_dispatcher()?;
        let compositor = checked(Compositor::new(), "create native compositor")?;
        let interop: ICompositorDesktopInterop = checked(compositor.cast(), "query desktop interop")?;
        // This selects a composition slot within this HWND, not WS_EX_TOPMOST.
        // Product new() always passes false because GPUI occupies the upper slot.
        let target = checked(unsafe { interop.CreateDesktopWindowTarget(hwnd, upper) }, "create composition target")?;
        let visual = checked(compositor.CreateSpriteVisual(), "create backdrop visual")?;
        let geometry = checked(compositor.CreateRoundedRectangleGeometry(), "create capsule geometry")?;
        let clip = checked(compositor.CreateGeometricClipWithGeometry(&geometry), "create compositor capsule clip")?;
        checked(visual.SetClip(&clip), "attach compositor capsule clip")?;
        let brush = checked(compositor.CreateHostBackdropBrush(), "create native host backdrop brush")?;
        checked(visual.SetBrush(&brush), "attach native host backdrop brush")?;
        if let Some(value) = opacity {
            // Only the explicit bare-host probe passes Some. Set once BEFORE
            // root attachment and window show, never as a refresh workaround.
            checked(visual.SetOpacity(value), "set diagnostic visual opacity")?;
        }
        let mut surface = Self { identity, target, compositor, visual, geometry, _opt_in: opt_in, probe: None };
        surface.resize(width, height)?;
        checked(surface.target.SetRoot(&surface.visual), "attach visual tree")?;
        // Explicit preview-only opt-in. Ordinary client/preview startup is unchanged.
        surface.probe = probe::Probe::from_args()?;
        eprintln!("[glass-host] native brush attached; compositor capsule clip; legacy-acrylic=off");
        Ok(surface)
    }

    pub fn identity(&self) -> isize { self.identity }

    pub fn has_armed_probe(&self) -> bool {
        self.probe.as_ref().is_some_and(probe::Probe::armed)
    }

    /// One diagnostic request, serviced on the owning GPUI UI thread. These are
    /// resource replacements, not a production recovery policy. Neither detaches
    /// the HWND, disables host backdrop, clears the root, nor redraws the source.
    pub fn service_probe(&mut self) -> Result<(), String> {
        let Some(mode) = (match self.probe.as_mut() {
            Some(probe) => probe.take_request()?, None => None,
        }) else { return Ok(()); };
        let started = std::time::Instant::now();
        let result = self.replace_probe_resource(mode);
        let signal = self.probe.as_ref().expect("active probe").finish(result.is_ok());
        eprintln!("[glass-host-probe] mode={mode:?}; applied={}; call-us={}; error={:?}",
            result.is_ok(), started.elapsed().as_micros(), result.as_ref().err());
        // Acknowledgement means the API call returned, NOT that pixels presented.
        result.and(signal)
    }

    fn replace_probe_resource(&mut self, mode: probe::Mode) -> Result<(), String> {
        let brush = checked(self.compositor.CreateHostBackdropBrush(), "create replacement host brush")?;
        match mode {
            probe::Mode::Brush => checked(self.visual.SetBrush(&brush), "replace host brush"),
            probe::Mode::Visual => {
                let replacement = checked(self.compositor.CreateSpriteVisual(), "create replacement visual")?;
                checked(replacement.SetSize(checked(self.visual.Size(), "read existing size")?), "copy visual size")?;
                checked(replacement.SetOffset(checked(self.visual.Offset(), "read existing offset")?), "copy visual offset")?;
                checked(replacement.SetOpacity(checked(self.visual.Opacity(), "read existing opacity")?), "copy visual opacity")?;
                let clip = checked(self.visual.Clip(), "read existing capsule clip")?;
                checked(replacement.SetClip(&clip), "copy capsule clip")?;
                checked(replacement.SetBrush(&brush), "attach replacement brush")?;
                // Fully configure before replacing the root. Never present an
                // intentionally empty tree or an unclipped intermediate visual.
                checked(self.target.SetRoot(&replacement), "replace lower visual root")?;
                self.visual = replacement;
                Ok(())
            }
        }
    }

    pub fn resize(&mut self, width: u32, height: u32) -> Result<(), String> {
        let mut rect = RECT::default();
        checked(unsafe { GetClientRect(HWND(self.identity as *mut c_void), &mut rect) }, "read backdrop client geometry")?;
        let client_width = rect.right - rect.left;
        let client_height = rect.bottom - rect.top;
        if width == 0 || height == 0 || width > 4096 || height > 4096
            || width as i32 > client_width || height as i32 > client_height {
            return Err("backdrop dimensions are outside the client rectangle".into());
        }
        // Reuse the generated Numerics value types without adding another crate
        // or changing Cargo.lock. All these values are DEVICE pixels.
        let mut size = checked(self.visual.Size(), "read backdrop size")?;
        size.X = width as f32;
        size.Y = height as f32;
        checked(self.visual.SetSize(size), "size backdrop visual")?;
        checked(self.geometry.SetSize(size), "size capsule geometry")?;
        let mut radius = size;
        radius.X = width.min(height) as f32 / 2.0;
        radius.Y = radius.X;
        checked(self.geometry.SetCornerRadius(radius), "round capsule geometry")?;
        let mut offset = checked(self.visual.Offset(), "read backdrop offset")?;
        offset.X = ((client_width - width as i32) / 2) as f32;
        offset.Y = ((client_height - height as i32) / 2) as f32;
        offset.Z = 0.0;
        checked(self.visual.SetOffset(offset), "position backdrop visual")
    }
}

impl Drop for HostBackdrop {
    fn drop(&mut self) {
        // Remove the native visual before disabling its HWND opt-in. This does
        // not close the GPUI window or its separate top composition target.
        let _ = self.target.Close();
        let _ = self.compositor.Close();
    }
}
