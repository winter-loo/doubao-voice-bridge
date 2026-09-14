//! Shared native compositor setup. A successful request does not prove visible
//! blur: GPUI 0.2.2 exposes no effect-status result. Validate on the real desktop.

use gpui::Window;
use super::policy::Backend;

#[cfg(target_os = "windows")]
mod platform {
    use std::{cell::RefCell, ffi::c_void, sync::OnceLock, time::{Duration, Instant}};
    use gpui::{Window, WindowBackgroundAppearance};
    use raw_window_handle::{HasWindowHandle, RawWindowHandle};
    use super::Backend;

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

    // GPUI's manifest/UI thread is per-monitor DPI aware. The width and height
    // here are the SAME device-pixel size passed to the actual texture renderer.
    // Window rectangles include invisible borders; dividing them by logical
    // overlay dimensions produces an oversized blur region on some Windows builds.
    fn clip(hwnd: isize, width: u32, height: u32) -> bool {
        if width == 0 || height == 0 || width > 4096 || height > 4096 { return false; }
        let mut outer = Rect::default();
        let mut client = Rect::default();
        let mut origin = Point::default();
        unsafe {
            if GetWindowRect(hwnd, &mut outer) == 0 || GetClientRect(hwnd, &mut client) == 0 || ClientToScreen(hwnd, &mut origin) == 0 { return false; }
            let left = origin.x - outer.left + (client.right - client.left - width as i32) / 2;
            let top = origin.y - outer.top + (client.bottom - client.top - height as i32) / 2;
            let region = CreateRoundRectRgn(left, top, left + width as i32, top + height as i32, height as i32, height as i32);
            if region == 0 { return false; }
            if SetWindowRgn(hwnd, region, 0) == 0 {
                DeleteObject(region);
                return false;
            }
            // Windows owns region after success.
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
    struct State { window: isize, size: (u32, u32), backend: Option<Backend>, checked: Option<Instant>, allowed: bool, clip_ok: bool }
    thread_local! { static STATE: RefCell<State> = RefCell::new(State::default()); }

    pub fn prepare(window: &Window, width: u32, height: u32) -> Backend {
        let identity = match HasWindowHandle::window_handle(window).map(|h| h.as_raw()) {
            Ok(RawWindowHandle::Win32(h)) => h.hwnd.get(), _ => return Backend::Solid,
        };
        let request = requested();
        let (backend, changed) = STATE.with(|state| {
            let mut state = state.borrow_mut();
            let geometry_changed = state.window != identity || state.size != (width, height);
            if geometry_changed {
                state.clip_ok = clip(identity, width, height);
                if !state.clip_ok { eprintln!("[glass] capsule clipping failed; native blur disabled"); }
            }
            if state.checked.is_none_or(|time| time.elapsed() >= Duration::from_secs(1)) {
                state.allowed = allows_native(); state.checked = Some(Instant::now());
            }
            let backend = request.resolve(state.allowed && state.clip_ok);
            let changed = geometry_changed || state.backend != Some(backend);
            state.window = identity; state.size = (width, height); state.backend = Some(backend);
            (backend, changed)
        });
        if changed {
            window.set_background_appearance(if backend == Backend::Native { WindowBackgroundAppearance::Blurred } else { WindowBackgroundAppearance::Transparent });
            eprintln!("[glass] requested={request:?} selected={backend:?}; hwnd=0x{identity:X}; pixels={width}x{height}");
        }
        backend
    }
}

pub fn prepare(window: &Window, width: u32, height: u32) -> Backend {
    #[cfg(target_os = "windows")]
    { platform::prepare(window, width, height) }
    #[cfg(not(target_os = "windows"))]
    { let _ = (window, width, height); Backend::Transparent }
}
