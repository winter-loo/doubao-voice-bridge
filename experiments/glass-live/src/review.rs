//! Review controls have no capture side effects. A pair is exported only after
//! an explicit, one-shot controller request in opt-in review mode.
#[allow(dead_code)]
pub const SAVE_PAIR_MESSAGE: u32 = 0x8000 + 0x31;
#[allow(dead_code)]
pub const CLOSE_REVIEW_MESSAGE: u32 = 0x8000 + 0x32;

#[derive(Default)]
#[allow(dead_code)]
pub struct Control {
    pub enabled: bool,
    pair_pending: bool,
    pair_used: bool,
    pub exit_reason: Option<&'static str>,
}
#[allow(dead_code)]
impl Control {
    pub fn new(enabled: bool) -> Self { Self { enabled, ..Self::default() } }
    pub fn right_click_closes(&self) -> bool { !self.enabled }
    pub fn request_pair(&mut self, request: usize) -> bool {
        if !self.enabled || request != 1 || self.pair_used { return false; }
        self.pair_used = true;
        self.pair_pending = true;
        true
    }
    pub fn take_pair(&mut self) -> bool { std::mem::take(&mut self.pair_pending) }
    pub fn stop(&mut self, reason: &'static str) { self.exit_reason.get_or_insert(reason); }
}

/// No capture, crop, prepare, Present or time advance occurs between these two
/// renders. Restore the selected theme even if a PNG write fails. The caller
/// publishes pair.json only after this function succeeds.
#[cfg(windows)]
pub unsafe fn save_pair(
    pipe: &crate::gpu::Pipeline, directory: &std::path::Path,
    selected_dark: bool, time: f32, foreground: bool,
) -> crate::AppResult<()> {
    let result = (|| -> crate::AppResult<()> {
        pipe.render(false, time, foreground);
        pipe.snapshot(directory, 1)?;
        pipe.render(true, time, foreground);
        pipe.snapshot(directory, 2)?;
        Ok(())
    })();
    pipe.render(selected_dark, time, foreground);
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn review_mouse_close_is_disabled_but_normal_mode_is_unchanged() {
        assert!(!Control::new(true).right_click_closes());
        assert!(Control::new(false).right_click_closes());
    }
    #[test]
    fn request_is_opt_in_one_shot_and_does_not_close() {
        let mut off = Control::new(false);
        assert!(!off.request_pair(1)); assert!(!off.take_pair());
        let mut on = Control::new(true);
        assert!(!on.request_pair(0)); assert!(!on.take_pair());
        assert!(on.request_pair(1)); assert!(!on.request_pair(1));
        assert!(on.take_pair()); assert!(!on.take_pair());
        assert!(!on.request_pair(1)); assert!(on.exit_reason.is_none());
    }
    #[test]
    fn_first_exit_cause_is_preserved() {
        let mut c = Control::new(true);
        c.stop("controller-close"); c.stop("WM_DESTROY"); c.stop("WM_QUIT");
        assert_eq!(c.exit_reason, Some("controller-close"));
    }
    #[cfg(windows)]
    #[test]
    fn pair_uses_same_gpu_background_and_restores_current_theme() {
        unsafe {
            let (device, context) = crate::gpu::create_device(None).unwrap();
            let pipe = crate::gpu::Pipeline::new(device, context, 160, 40, &vec![0; 160*40]).unwrap();
            let (w,h) = (pipe.raw.width, pipe.raw.height);
            let mut pixels = vec![0; (w*h*4) as usize];
            for y in 0..h { for x in 0..w {
                let c = if x < w/2 { [64,48,220,255] } else { [224,112,32,255] };
                pixels[((y*w+x)*4) as usize..][..4].copy_from_slice(&c);
            } }
            pipe.context.UpdateSubresource(&pipe.raw.texture,0,None,pixels.as_ptr().cast(),w*4,0);
            pipe.prepare(); pipe.render(true, 1.25, false);
            let before = pipe.read_rgba(&pipe.output).unwrap();
            let raw_before = pipe.read_rgba(&pipe.raw).unwrap();
            let stamp = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos();
            let dir = std::env::temp_dir().join(format!("glass-review-test-{}-{stamp}",std::process::id()));
            std::fs::create_dir(&dir).unwrap();
            save_pair(&pipe,&dir,true,1.25,false).unwrap();
            assert_eq!(before,pipe.read_rgba(&pipe.output).unwrap());
            assert_eq!(raw_before,pipe.read_rgba(&pipe.raw).unwrap());
            let light = std::fs::read(dir.join("snapshot-0001.png")).unwrap();
            let dark = std::fs::read(dir.join("snapshot-0002.png")).unwrap();
            assert!(light.starts_with(b"\x89PNG\r\n\x1a\n"));
            assert!(dark.starts_with(b"\x89PNG\r\n\x1a\n")); assert_ne!(light,dark);
            // Existing paths cause a refusal, not overwriting, and still restore.
            assert!(save_pair(&pipe,&dir,true,1.25,false).is_err());
            assert_eq!(before,pipe.read_rgba(&pipe.output).unwrap());
            std::fs::remove_dir_all(&dir).unwrap();
        }
    }
}
