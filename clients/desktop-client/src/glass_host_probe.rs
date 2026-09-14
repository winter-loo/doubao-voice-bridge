//! Test-only control channel. Dormant unless GlassPreview.exe is explicitly
//! started with --glass-probe=brush or --glass-probe=visual. Never polls files,
//! captures pixels, hooks foreign windows, or schedules periodic recovery.

use std::ffi::c_void;

#[derive(Clone, Copy, Debug)]
pub enum Mode { Brush, Visual }

#[link(name = "kernel32")]
unsafe extern "system" {
    fn CreateEventW(attributes: *const c_void, manual: i32, initial: i32, name: *const u16) -> *mut c_void;
    fn WaitForSingleObject(handle: *mut c_void, milliseconds: u32) -> u32;
    fn SetEvent(handle: *mut c_void) -> i32;
    fn CloseHandle(handle: *mut c_void) -> i32;
}

struct Event(*mut c_void);
impl Event {
    fn new(suffix: &str, manual: bool) -> Result<Self, String> {
        let name: Vec<u16> = format!("Local\\DoubaoGlassProbe.{}.{}\0", std::process::id(), suffix)
            .encode_utf16().collect();
        let raw = unsafe { CreateEventW(std::ptr::null(), i32::from(manual), 0, name.as_ptr()) };
        let error = std::io::Error::last_os_error();
        if raw.is_null() { return Err(format!("create preview probe event: {error}")); }
        let event = Self(raw);
        if error.raw_os_error() == Some(183) {
            return Err("preview probe event already exists; refusing ambiguous control".into());
        }
        Ok(event)
    }

    fn signal(&self) -> Result<(), String> {
        if unsafe { SetEvent(self.0) } == 0 {
            return Err(format!("signal preview probe: {}", std::io::Error::last_os_error()));
        }
        Ok(())
    }
}
impl Drop for Event {
    fn drop(&mut self) { let _ = unsafe { CloseHandle(self.0) }; }
}

pub struct Probe {
    mode: Mode,
    request: Event,
    applied: Event,
    failed: Event,
    attempted: bool,
}

impl Probe {
    pub fn from_args() -> Result<Option<Self>, String> {
        let Some(value) = std::env::args().find_map(|arg| arg.strip_prefix("--glass-probe=").map(str::to_owned)) else {
            return Ok(None);
        };
        let executable = std::env::current_exe().map_err(|e| e.to_string())?;
        if !executable.file_name().is_some_and(|name| name.to_string_lossy().eq_ignore_ascii_case("GlassPreview.exe")) {
            return Err("compositor probe is restricted to the standalone GlassPreview.exe".into());
        }
        let mode = match value.as_str() {
            "brush" => Mode::Brush,
            "visual" => Mode::Visual,
            _ => return Err("--glass-probe must be brush or visual".into()),
        };
        let probe = Self {
            mode, request: Event::new("request", false)?, applied: Event::new("applied", true)?,
            failed: Event::new("failed", true)?, attempted: false,
        };
        eprintln!("[glass-host-probe] ready mode={mode:?}; one-shot; no automatic recovery");
        Ok(Some(probe))
    }

    pub fn armed(&self) -> bool { !self.attempted }

    pub fn take_request(&mut self) -> Result<Option<Mode>, String> {
        if self.attempted { return Ok(None); }
        match unsafe { WaitForSingleObject(self.request.0, 0) } {
            0 => { self.attempted = true; Ok(Some(self.mode)) }
            258 => Ok(None),
            _ => {
                self.attempted = true;
                let _ = self.failed.signal();
                Err(format!("poll preview probe: {}", std::io::Error::last_os_error()))
            }
        }
    }

    pub fn finish(&self, ok: bool) -> Result<(), String> {
        if ok { self.applied.signal() } else { self.failed.signal() }
    }
}
