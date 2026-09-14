//! Reduction baseline, NOT a replacement voice client or a visual design.
//! No GPUI Application, renderer, texture baking, animation, capture, or voice.
//! Default 'host' retains the product's lower-target HostBackdrop setup.
//! 'host-upper' changes ONLY the composition target slot in this bare HWND;
//! it does not alter WS_EX_TOPMOST, and it cannot occupy GPUI's target.
//! 'visual-blur' selects standard backdrop + system Gaussian on the lower slot.
//! 'none' creates NO compositor, target, visual, brush or host opt-in.
//! Compare startup to warm pixels WITHIN each mode; a constant output is not a pass.

// windows-implement expands absolute ::windows_core paths. This diagnostic-only
// crate facade reexports the EXACT runtime used by windows 0.61; it introduces
// no second windows-core dependency/version or product lockfile change.
#[cfg(target_os = "windows")]
extern crate self as windows_core;
#[cfg(target_os = "windows")]
pub use windows::core::*;

#[cfg(target_os = "windows")]
#[allow(dead_code)]
#[path = "../src/glass_host.rs"]
mod glass_host;
#[cfg(target_os = "windows")]
#[path = "support/visual_blur.rs"]
mod visual_blur;

#[cfg(target_os = "windows")]
mod native {
    use std::{ffi::c_void, mem::size_of, ptr, time::{Duration, Instant}};
    use windows::{System::DispatcherQueueController, Win32::System::WinRT::{
        CreateDispatcherQueueController, DispatcherQueueOptions,
        DQTAT_COM_NONE, DQTYPE_THREAD_CURRENT,
    }};
    use super::{glass_host::HostBackdrop, visual_blur::VisualBlur};

    type WndProc = unsafe extern "system" fn(isize, u32, usize, isize) -> isize;
    #[repr(C)]
    struct Class {
        size: u32, style: u32, proc: Option<WndProc>, class_extra: i32, window_extra: i32,
        instance: isize, icon: isize, cursor: isize, background: isize,
        menu: *const u16, name: *const u16, small_icon: isize,
    }
    #[repr(C)]
    #[derive(Default)]
    struct Point { x: i32, y: i32 }
    #[repr(C)]
    #[derive(Default)]
    struct Rect { left: i32, top: i32, right: i32, bottom: i32 }
    #[repr(C)]
    #[derive(Default)]
    struct Message { hwnd: isize, id: u32, wparam: usize, lparam: isize, time: u32, point: Point, private: u32 }
    #[repr(C)]
    struct Paint { dc: isize, erase: i32, rect: Rect, restore: i32, update: i32, reserved: [u8; 32] }

    #[link(name = "user32")]
    unsafe extern "system" {
        fn RegisterClassExW(class: *const Class) -> u16;
        fn UnregisterClassW(name: *const u16, instance: isize) -> i32;
        fn CreateWindowExW(ex: u32, class: *const u16, title: *const u16, style: u32,
            x: i32, y: i32, w: i32, h: i32, parent: isize, menu: isize, instance: isize, arg: *const c_void) -> isize;
        fn DestroyWindow(hwnd: isize) -> i32;
        fn DefWindowProcW(hwnd: isize, msg: u32, w: usize, l: isize) -> isize;
        fn GetMessageW(msg: *mut Message, hwnd: isize, min: u32, max: u32) -> i32;
        fn PeekMessageW(msg: *mut Message, hwnd: isize, min: u32, max: u32, flags: u32) -> i32;
        fn TranslateMessage(msg: *const Message) -> i32;
        fn DispatchMessageW(msg: *const Message) -> isize;
        fn PostQuitMessage(code: i32);
        fn SetThreadDpiAwarenessContext(context: isize) -> isize;
        fn GetDpiForWindow(hwnd: isize) -> u32;
        fn GetSystemMetrics(index: i32) -> i32;
        fn SetWindowPos(hwnd: isize, after: isize, x: i32, y: i32, w: i32, h: i32, flags: u32) -> i32;
        fn SetWindowRgn(hwnd: isize, region: isize, redraw: i32) -> i32;
        fn SetTimer(hwnd: isize, id: usize, ms: u32, callback: *const c_void) -> usize;
        fn BeginPaint(hwnd: isize, paint: *mut Paint) -> isize;
        fn EndPaint(hwnd: isize, paint: *const Paint) -> i32;
    }
    #[link(name = "gdi32")]
    unsafe extern "system" {
        fn CreateRoundRectRgn(l: i32, t: i32, r: i32, b: i32, ew: i32, eh: i32) -> isize;
        fn DeleteObject(object: isize) -> i32;
    }
    #[link(name = "kernel32")]
    unsafe extern "system" { fn GetModuleHandleW(name: *const u16) -> isize; }
    #[link(name = "runtimeobject")]
    unsafe extern "system" { fn RoInitialize(kind: u32) -> i32; fn RoUninitialize(); }

    fn require(ok: bool, operation: &str) -> Result<(), String> {
        if ok { Ok(()) } else { Err(format!("{operation}: {}", std::io::Error::last_os_error())) }
    }
    fn wide(value: &str) -> Vec<u16> { value.encode_utf16().chain(Some(0)).collect() }
    struct Apartment;
    impl Drop for Apartment { fn drop(&mut self) { unsafe { RoUninitialize(); } } }
    struct Dpi(isize);
    impl Drop for Dpi { fn drop(&mut self) { unsafe { SetThreadDpiAwarenessContext(self.0); } } }
    struct Window(isize);
    impl Drop for Window { fn drop(&mut self) { unsafe { DestroyWindow(self.0); } } }
    struct Registration { name: Vec<u16>, instance: isize }
    impl Drop for Registration {
        fn drop(&mut self) { unsafe { UnregisterClassW(self.name.as_ptr(), self.instance); } }
    }

    unsafe extern "system" fn procedure(hwnd: isize, msg: u32, w: usize, l: isize) -> isize {
        match msg {
            0x0010 | 0x0202 => { unsafe { PostQuitMessage(0); } 0 }
            0x0113 if w == 1 => { unsafe { PostQuitMessage(0); } 0 }
            0x0014 => 1,
            0x000F => {
                let mut paint: Paint = unsafe { std::mem::zeroed() };
                if unsafe { BeginPaint(hwnd, &mut paint) } != 0 {
                    unsafe { EndPaint(hwnd, &paint); }
                }
                0
            }
            _ => unsafe { DefWindowProcW(hwnd, msg, w, l) },
        }
    }

    fn window_loop(seconds: u32, source: &str) -> Result<(), String> {
        let instance = unsafe { GetModuleHandleW(ptr::null()) };
        require(instance != 0, "get module")?;
        let name = wide("Doubao::NativeHostReduction");
        let class = Class {
            size: size_of::<Class>() as u32, style: 0, proc: Some(procedure), class_extra: 0, window_extra: 0,
            instance, icon: 0, cursor: 0, background: 0, menu: ptr::null(), name: name.as_ptr(), small_icon: 0,
        };
        require(unsafe { RegisterClassExW(&class) } != 0, "register reduction window")?;
        let registration = Registration { name, instance };
        let title = wide("Doubao Glass Preview - no microphone");
        let sw = unsafe { GetSystemMetrics(0) };
        let sh = unsafe { GetSystemMetrics(1) };
        require(sw >= 640 && sh >= 480, "primary monitor dimensions")?;
        // TOPMOST | TOOLWINDOW | NOACTIVATE | NOREDIRECTIONBITMAP. These stay
        // identical in both target-slot arms. No accent, layered alpha or GDI fill.
        let ex_style = 0x08200088;
        let raw = unsafe { CreateWindowExW(ex_style, registration.name.as_ptr(), title.as_ptr(),
            0x80000000, sw / 2 - 64, sh - 180, 128, 42, 0, 0, instance, ptr::null()) };
        require(raw != 0, "create reduction window")?;
        let window = Window(raw);
        let dpi = unsafe { GetDpiForWindow(raw) };
        require(dpi != 0, "read window DPI")?;
        let px = |v: f64| (v * dpi as f64 / 96.0).round() as i32;
        let (cw, ch, width, height) = (px(128.0), px(42.0), px(108.0), px(26.0));
        require(unsafe { SetWindowPos(raw, -1, (sw-cw)/2, sh-ch-px(60.0), cw, ch, 0x0030) } != 0,
            "position reduction window")?;
        let (left, top) = ((cw-width)/2, (ch-height)/2);
        let region = unsafe { CreateRoundRectRgn(left, top, left+width, top+height, height, height) };
        require(region != 0, "create capsule HRGN")?;
        if unsafe { SetWindowRgn(raw, region, 1) } == 0 {
            unsafe { DeleteObject(region); }
            return Err("install reduction capsule region".into());
        }
        // Both host arms call exactly the same shared constructor and verify
        // DesktopWindowTarget.IsTopmost. Only its creation-time bool differs.
        // The product still calls HostBackdrop::new(), fixed to lower.
        let surface = match source {
            "host" | "host-upper" => (Some(HostBackdrop::new_for_target_probe(
                raw, width as u32, height as u32, source == "host-upper",
            )?), None),
            "visual-blur" => (None, Some(VisualBlur::new(raw, width as u32, height as u32)?)),
            "none" => {
                eprintln!("[glass-clear] compositor=absent; target=absent; visual=absent; brush=absent; host-opt-in=not-enabled");
                (None, None)
            }
            _ => return Err("invalid backdrop source".into()),
        };
        let selected = if source == "none" { "Transparent" } else { "Native" };
        let target_slot = match source { "host-upper" => "upper", "none" => "absent", _ => "lower" };
        require(unsafe { SetTimer(raw, 1, seconds * 1000, ptr::null()) } != 0, "set lifetime timer")?;
        require(unsafe { SetWindowPos(raw, -1, 0, 0, 0, 0, 0x0053) } != 0, "show without activation")?;
        eprintln!("[glass-minimal] selected={selected}; pid={}; hwnd=0x{raw:X}; dpi={dpi}; pixels={width}x{height}; GPUI=absent; foreground-target=absent; source={source}; target-slot={target_slot}; requested-theme-not-applied; no microphone", std::process::id());
        let outcome = loop {
            let mut message = Message::default();
            let status = unsafe { GetMessageW(&mut message, 0, 0, 0) };
            if status == 0 { break Ok(()); }
            if status < 0 { break Err(format!("message loop: {}", std::io::Error::last_os_error())); }
            unsafe { TranslateMessage(&message); DispatchMessageW(&message); }
        };
        drop(surface);
        drop(window);
        outcome
    }

    fn shutdown(queue: &DispatcherQueueController) -> Result<(), String> {
        let action = queue.ShutdownQueueAsync().map_err(|e| e.to_string())?;
        let timer = Instant::now();
        while action.Status().map_err(|e| e.to_string())?.0 == 0 {
            if timer.elapsed() > Duration::from_secs(2) { return Err("dispatcher shutdown timed out".into()); }
            let mut message = Message::default();
            while unsafe { PeekMessageW(&mut message, 0, 0, 0, 1) } != 0 {
                if message.id != 0x0012 { unsafe { TranslateMessage(&message); DispatchMessageW(&message); } }
            }
            std::thread::sleep(Duration::from_millis(5));
        }
        action.GetResults().map_err(|e| e.to_string())
    }

    pub fn run() -> Result<(), String> {
        require(size_of::<usize>() == 8, "64-bit baseline required")?;
        let mut seconds = 60;
        let mut source = String::from("host");
        for arg in std::env::args().skip(1) {
            if let Some(value) = arg.strip_prefix("--seconds=") {
                seconds = value.parse::<u32>().map_err(|_| "invalid lifetime")?;
                if !(5..=120).contains(&seconds) { return Err("lifetime must be 5..120 seconds".into()); }
            } else if let Some(value) = arg.strip_prefix("--backdrop-source=") {
                if !matches!(value, "host" | "host-upper" | "visual-blur" | "none") {
                    return Err("backdrop source must be host, host-upper, visual-blur or none".into());
                }
                source = value.to_owned();
            } else if !matches!(arg.as_str(), "--glass-backend=native" | "--theme=dark" | "--theme=light" | "--content=optimizing") {
                return Err(format!("unsupported baseline argument: {arg}"));
            }
        }
        let hr = unsafe { RoInitialize(0) };
        if hr < 0 { return Err(format!("initialize WinRT: 0x{:08X}", hr as u32)); }
        let _apartment = Apartment;
        let previous = unsafe { SetThreadDpiAwarenessContext(-4) };
        require(previous != 0, "set per-monitor physical-pixel DPI context")?;
        let _dpi = Dpi(previous);
        let queue = unsafe { CreateDispatcherQueueController(DispatcherQueueOptions {
            dwSize: size_of::<DispatcherQueueOptions>() as u32,
            threadType: DQTYPE_THREAD_CURRENT, apartmentType: DQTAT_COM_NONE,
        }) }.map_err(|e| e.to_string())?;
        let result = window_loop(seconds, &source);
        let closed = shutdown(&queue);
        drop(queue);
        result.and(closed)
    }
}

#[cfg(target_os = "windows")]
fn main() {
    if let Err(error) = native::run() {
        eprintln!("[glass-minimal] failed: {error}");
        std::process::exit(1);
    }
}
#[cfg(not(target_os = "windows"))]
fn main() { eprintln!("GlassNativeMinimal requires a Windows 11 desktop; no test was run."); std::process::exit(2); }
