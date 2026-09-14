//! Native material selection shared by production and GlassPreview.
//!
//! The HWND Acrylic accent ignored the correctly installed capsule HRGN in our
//! Windows 11 / GPUI 0.2.2 test. Native now uses a separately clipped composition
//! visual. An unavailable visual fails closed to solid, never to clear glass or
//! the known-overflowing Acrylic accent. Visual acceptance is still required.

use gpui::Window;
use super::policy::Backend;
#[cfg(target_os = "windows")]
#[path = "glass_host.rs"]
mod host;

#[cfg(target_os = "windows")]
mod platform {
    use std::{cell::RefCell, ffi::c_void, sync::OnceLock, time::{Duration, Instant}};
    use gpui::{Window, WindowBackgroundAppearance};
    use raw_window_handle::{HasWindowHandle, RawWindowHandle};
    use super::{Backend, host::HostBackdrop};

    #[repr(C)]
    struct HighContrast { size: u32, flags: u32, scheme: *mut u16 }
    #[repr(C)]
    #[derive(Default)]
    struct Rect { left: i32, top: i32, right: i32, bottom: i32 }
    #[repr(C)]
    #[derive(Default)]
    struct Point { x: i32, y: i32 }
    #[link(name = "user32")]
    unsafe extern "system" {
        fn SystemParametersInfoW(action: u32, param: u32, value: *mut c_void, flags: u32) -> i32;
        fn GetSystemMetrics(index: i32) -> i32;
        fn GetWindowRect(hwnd: isize, rect: *mut Rect) -> i32;
        fn GetClientRect(hwnd: isize, rect: *mut Rect) -> i32;
        fn ClientToScreen(hwnd: isize, point: *mut Point) -> i32;
        fn SetWindowRgn(hwnd: isize, region: isize, redraw: i32) -> i32;
    }
    #[link(name = "gdi32")]
    unsafe extern "system" {
        fn CreateRoundRectRgn(left: i32, top: i32, right: i32, bottom: i32, ellipse_width: i32, ellipse_height: i32) -> isize;
        fn DeleteObject(object: isize) -> i32;
    }
    #[link(name = "advapi32")]
    unsafe extern "system" {
        fn RegGetValueW(key: *mut c_void, subkey: *const u16, value: *const u16, flags: u32, kind: *mut u32, data: *mut c_void, size: *mut u32) -> i32;
    }

    // Keep hit testing and the GPUI foreground limited to the capsule. This is
    // NOT the native blur clip; glass_host installs that in the compositor tree.
    fn clip(hwnd: isize, width: u32, height: u32) -> bool {
        if width == 0 || height == 0 || width > 4096 || height > 4096 { return false; }
        let mut outer = Rect::default();
        let mut client = Rect::default();
        let mut origin = Point::default();
        unsafe {
            if GetWindowRect(hwnd, &mut outer) == 0 || GetClientRect(hwnd, &mut client) == 0 || ClientToScreen(hwnd, &mut origin) == 0 { return false; }
            if width as i32 > client.right - client.left || height as i32 > client.bottom - client.top { return false; }
            let left = origin.x - outer.left + (client.right - client.left - width as i32) / 2;
            let top = origin.y - outer.top + (client.bottom - client.top - height as i32) / 2;
            let region = CreateRoundRectRgn(left, top, left + width as i32, top + height as i32, height as i32, height as i32);
            if region == 0 { return false; }
            if SetWindowRgn(hwnd, region, 0) == 0 {
                DeleteObject(region);
                return false;
            }
        }
        true
    }

    fn allows_native() -> bool {
        let mut contrast = HighContrast { size: std::mem::size_of::<HighContrast>() as u32, flags: 0, scheme: std::ptr::null_mut() };
        if unsafe { SystemParametersInfoW(0x0042, contrast.size, (&mut contrast as *mut HighContrast).cast(), 0) } == 0
            || contrast.flags & 1 != 0 || unsafe { GetSystemMetrics(0x1000) } != 0 { return false; }
        let key: Vec<u16> = "Software\\Microsoft\\Windows\\CurrentVersion\\Themes\\Personalize\0".encode_utf16().collect();
        let value: Vec<u16> = "EnableTransparency\0".encode_utf16().collect();
        let mut enabled = 1u32;
        let mut size = 4u32;
        let status = unsafe { RegGetValueW(
            (0x80000001u32 as i32 as isize) as *mut c_void, key.as_ptr(), value.as_ptr(),
            0x10, std::ptr::null_mut(), (&mut enabled as *mut u32).cast(), &mut size,
        ) };
        status == 2 || (status == 0 && enabled != 0)
    }

    fn requested() -> Backend {
        static REQUEST: OnceLock<Backend> = OnceLock::new();
        *REQUEST.get_or_init(|| {
            let value = std::env::args().find_map(|arg| arg.strip_prefix("--glass-backend=").map(str::to_owned))
                .or_else(|| std::env::var("DOUBAO_GLASS_BACKEND").ok()).unwrap_or_else(|| "native".into());
            Backend::parse(&value).unwrap_or_else(|error| { eprintln!("[glass] {error}; using solid"); Backend::Solid })
        })
    }
    #[derive(Default)]
    struct State {
        window: isize,
        size: (u32, u32),
        backend: Option<Backend>,
        checked: Option<Instant>,
        allowed: bool,
        clip_ok: bool,
        host_failed: bool,
        host: Option<HostBackdrop>,
    }
    thread_local! { static STATE: RefCell<State> = RefCell::new(State::default()); }

    pub fn prepare(window: &Window, width: u32, height: u32) -> Backend {
        let identity = match HasWindowHandle::window_handle(window).map(|h| h.as_raw()) {
            Ok(RawWindowHandle::Win32(h)) => h.hwnd.get(), _ => return Backend::Solid,
        };
        let request = requested();
        let (candidate, changed, mut host) = STATE.with(|cell| {
            let mut state = cell.borrow_mut();
            let identity_changed = state.window != identity;
            let geometry_changed = identity_changed || state.size != (width, height);
            if identity_changed { state.host_failed = false; }
            if geometry_changed {
                state.clip_ok = clip(identity, width, height);
                if !state.clip_ok { eprintln!("[glass] capsule clipping failed; native disabled"); }
            }
            if state.checked.is_none_or(|time| time.elapsed() >= Duration::from_secs(1)) {
                state.allowed = allows_native(); state.checked = Some(Instant::now());
            }
            let candidate = request.resolve(state.allowed && state.clip_ok && !state.host_failed);
            let changed = geometry_changed || state.backend != Some(candidate);
            state.window = identity;
            state.size = (width, height);
            // Dormant for ordinary startup. The opt-in standalone preview can
            // service one zero-wait diagnostic event without borrowing across COM.
            let probing = !changed && state.host.as_ref().is_some_and(HostBackdrop::has_armed_probe);
            let host = if changed || probing { state.host.take() } else { None };
            (candidate, changed, host)
        });
        if !changed {
            if let Some(mut surface) = host.take() {
                if let Err(error) = surface.service_probe() {
                    eprintln!("[glass-host-probe] failed: {error}");
                }
                STATE.with(|cell| {
                    let mut state = cell.borrow_mut();
                    // Do not overwrite a new state in the event of reentrant work.
                    if state.window == identity && state.size == (width, height)
                        && state.backend == Some(candidate) && state.host.is_none() {
                        state.host = Some(surface);
                    }
                });
            }
            return candidate;
        }

        // Do WinRT/GPUI calls outside the RefCell borrow (callbacks may re-enter).
        if host.as_ref().is_some_and(|surface| surface.identity() != identity) {
            host = None;
        }
        let mut selected = candidate;
        let mut failed = false;
        if candidate == Backend::Native {
            let result = if let Some(surface) = host.as_mut() {
                // Do not reset the HWND accent or host opt-in on a size change.
                surface.resize(width, height)
            } else {
                // GPUI 0.2.2 Windows maps Transparent to legacy accent state 2,
                // NOT to absence of a window effect. Opaque maps to state 0
                // (ACCENT_DISABLED); this setter does not change the renderer's
                // alpha-preserving DirectComposition swap chain or paint a fill.
                // Clear the legacy accent once BEFORE enabling HostBackdrop.
                // This is version/platform-specific, not a portable recipe.
                window.set_background_appearance(WindowBackgroundAppearance::Opaque);
                eprintln!("[glass] host initialization: legacy-accent=disabled; GPUI composition unchanged");
                HostBackdrop::new(identity, width, height).map(|surface| { host = Some(surface); })
            };
            if let Err(error) = result {
                // Latch this failure for this window instead of retrying every
                // animation frame or ever exposing the broken Acrylic rectangle.
                eprintln!("[glass] native visual unavailable; using solid: {error}");
                host = None;
                // Preserve the previously tested Solid rendering configuration.
                window.set_background_appearance(WindowBackgroundAppearance::Transparent);
                failed = true;
                selected = Backend::Solid;
            }
        } else {
            // Detach the native visual before restoring the non-native setup.
            host = None;
            window.set_background_appearance(WindowBackgroundAppearance::Transparent);
        }
        STATE.with(|cell| {
            let mut state = cell.borrow_mut();
            state.host = host;
            state.host_failed |= failed;
            state.backend = Some(selected);
        });
        let path = if selected == Backend::Native { "clipped-host-visual" } else { "off" };
        eprintln!("[glass] requested={request:?} selected={selected:?}; hwnd=0x{identity:X}; pixels={width}x{height}; native-path={path}");
        selected
    }
}

pub fn prepare(window: &Window, width: u32, height: u32) -> Backend {
    #[cfg(target_os = "windows")]
    { platform::prepare(window, width, height) }
    #[cfg(not(target_os = "windows"))]
    { let _ = (window, width, height); Backend::Transparent }
}
