//! A native backdrop VISUAL, not an Acrylic accent attached to the whole HWND.
//!
//! GPUI 0.2.2 owns the HWND's top DirectComposition target. This module uses the
//! lower target and clips its own brush in the composition tree. It does not
//! capture the desktop, read background pixels, or touch another process.
//! Windows 11's documented host-backdrop opt-in is required; callers fail closed
//! to the solid material when any part of initialization is unavailable.

use std::{cell::RefCell, ffi::c_void};
use windows::{
    System::{DispatcherQueue, DispatcherQueueController},
    UI::Composition::{
        CompositionRoundedRectangleGeometry, Compositor, Desktop::DesktopWindowTarget,
        SpriteVisual,
    },
    Win32::{
        Foundation::{BOOL, HWND, RECT},
        Graphics::Dwm::{DWMWA_USE_HOSTBACKDROPBRUSH, DwmSetWindowAttribute},
        System::WinRT::{
            Composition::ICompositorDesktopInterop, CreateDispatcherQueueController,
            DQTAT_COM_NONE, DQTYPE_THREAD_CURRENT, DispatcherQueueOptions,
        },
        UI::WindowsAndMessaging::GetClientRect,
    },
    core::Interface,
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
}

impl HostBackdrop {
    pub fn new(identity: isize, width: u32, height: u32) -> Result<Self, String> {
        if std::env::var_os("GPUI_DISABLE_DIRECT_COMPOSITION").is_some() {
            return Err("host backdrop requires GPUI's alpha-preserving composition path".into());
        }
        let hwnd = HWND(identity as *mut c_void);
        let opt_in = HostOptIn::new(hwnd)?;
        ensure_dispatcher()?;
        let compositor = checked(Compositor::new(), "create native compositor")?;
        let interop: ICompositorDesktopInterop = checked(compositor.cast(), "query desktop interop")?;
        // GPUI uses CreateTargetForHwnd(hwnd, true). Keep its foreground tree
        // untouched; the false target is behind GPUI but above the HWND surface.
        let target = checked(unsafe { interop.CreateDesktopWindowTarget(hwnd, false) }, "create lower composition target")?;
        let visual = checked(compositor.CreateSpriteVisual(), "create backdrop visual")?;
        let geometry = checked(compositor.CreateRoundedRectangleGeometry(), "create capsule geometry")?;
        let clip = checked(compositor.CreateGeometricClipWithGeometry(&geometry), "create compositor capsule clip")?;
        checked(visual.SetClip(&clip), "attach compositor capsule clip")?;
        let brush = checked(compositor.CreateHostBackdropBrush(), "create native host backdrop brush")?;
        checked(visual.SetBrush(&brush), "attach native host backdrop brush")?;
        let mut surface = Self { identity, target, compositor, visual, geometry, _opt_in: opt_in };
        surface.resize(width, height)?;
        checked(surface.target.SetRoot(&surface.visual), "attach lower visual tree")?;
        eprintln!("[glass-host] native brush attached; compositor capsule clip; legacy-acrylic=off");
        Ok(surface)
    }

    pub fn identity(&self) -> isize { self.identity }

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
