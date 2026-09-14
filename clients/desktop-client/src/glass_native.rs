//! Native background selection. This is called on the UI thread by the existing
//! texture entrypoint, so both the production overlay and the no-mic preview use
//! the same path. GPUI's setter returns no effect status: requesting blur is NOT
//! proof that the compositor actually blurred it. Visual acceptance is required.

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
    #[link(name = "user32")]
    unsafe extern "system" {
        fn SystemParametersInfoW(action: u32, param: u32, value: *mut c_void, flags: u32) -> i32;
        fn GetSystemMetrics(index: i32) -> i32;
    }
    #[link(name = "advapi32")]
    unsafe extern "system" {
        fn RegGetValueW(key: *mut c_void, subkey: *const u16, value: *const u16, flags: u32, kind: *mut u32, data: *mut c_void, size: *mut u32) -> i32;
    }

    fn allows_native() -> bool {
        let mut contrast = HighContrast { size: std::mem::size_of::<HighContrast>() as u32, flags: 0, scheme: std::ptr::null_mut() };
        // Fail closed for an unreadable accessibility setting or a remote session.
        if unsafe { SystemParametersInfoW(0x0042, contrast.size, (&mut contrast as *mut HighContrast).cast(), 0) } == 0
            || contrast.flags & 1 != 0 || unsafe { GetSystemMetrics(0x1000) } != 0 {
            return false;
        }
        let key: Vec<u16> = "Software\\Microsoft\\Windows\\CurrentVersion\\Themes\\Personalize\0".encode_utf16().collect();
        let value: Vec<u16> = "EnableTransparency\0".encode_utf16().collect();
        let mut enabled = 1u32;
        let mut size = 4u32;
        let status = unsafe { RegGetValueW(
            (0x80000001u32 as i32 as isize) as *mut c_void,
            key.as_ptr(), value.as_ptr(), 0x10, std::ptr::null_mut(),
            (&mut enabled as *mut u32).cast(), &mut size,
        ) };
        // A missing value means the OS default. Other registry errors use solid.
        status == 2 || (status == 0 && enabled != 0)
    }

    fn requested() -> Backend {
        static REQUEST: OnceLock<Backend> = OnceLock::new();
        *REQUEST.get_or_init(|| {
            let value = std::env::args().find_map(|arg| arg.strip_prefix("--glass-backend=").map(str::to_owned))
                .or_else(|| std::env::var("DOUBAO_GLASS_BACKEND").ok())
                .unwrap_or_else(|| "native".into());
            Backend::parse(&value).unwrap_or_else(|error| {
                eprintln!("[glass] {error}; using solid");
                Backend::Solid
            })
        })
    }

    #[derive(Default)]
    struct State { window: isize, backend: Option<Backend>, checked: Option<Instant>, allowed: bool }
    thread_local! { static STATE: RefCell<State> = RefCell::new(State::default()); }

    pub fn prepare(window: &Window) -> Backend {
        let identity = match HasWindowHandle::window_handle(window).map(|h| h.as_raw()) {
            Ok(RawWindowHandle::Win32(h)) => h.hwnd.get(),
            _ => return Backend::Solid,
        };
        let request = requested();
        let (backend, changed) = STATE.with(|state| {
            let mut state = state.borrow_mut();
            if state.checked.is_none_or(|time| time.elapsed() >= Duration::from_secs(1)) {
                state.allowed = allows_native();
                state.checked = Some(Instant::now());
            }
            let backend = request.resolve(state.allowed);
            let changed = state.window != identity || state.backend != Some(backend);
            state.window = identity;
            state.backend = Some(backend);
            (backend, changed)
        });
        if changed {
            window.set_background_appearance(if backend == Backend::Native {
                WindowBackgroundAppearance::Blurred
            } else {
                // Solid pixels are clipped by the same capsule mask, not by a
                // rectangular, opaque window-sized panel.
                WindowBackgroundAppearance::Transparent
            });
            eprintln!("[glass] requested={request:?} selected={backend:?}; hwnd=0x{identity:X}");
        }
        backend
    }
}

pub fn prepare(window: &Window) -> Backend {
    #[cfg(target_os = "windows")]
    { platform::prepare(window) }
    #[cfg(not(target_os = "windows"))]
    { let _ = window; Backend::Transparent }
}
