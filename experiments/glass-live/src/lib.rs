//! Shared optical renderer and in-process voice overlay. No microphone, network or global hotkeys.
//! Live capture requires explicit consent. Self-test never captures the desktop.
#![allow(unsafe_op_in_unsafe_fn)]

mod png;
mod review;
pub mod voice_model;
#[cfg(windows)]
pub mod voice_overlay;
#[cfg(windows)]
mod voice_window;
#[cfg(windows)]
#[path = "liquid_gpu.rs"]
mod gpu;
#[cfg(windows)]
mod desktop;

pub type AppResult<T> = Result<T, Box<dyn std::error::Error>>;
pub fn ensure(condition: bool, message: &str) -> AppResult<()> {
    if condition { Ok(()) } else { Err(message.into()) }
}

#[derive(Debug)]
pub struct Options {
    pub allow_capture: bool,
    pub self_test: bool,
    pub compact: bool,
    pub dark: bool,
    pub review_mode: bool,
    pub seconds: u64,
    pub snapshots: Option<std::path::PathBuf>,
}
impl Options {
    fn parse(args: impl Iterator<Item = String>) -> AppResult<Self> {
        let mut o = Self { allow_capture: false, self_test: false, compact: false,
            dark: false, review_mode: false, seconds: 600, snapshots: None };
        for arg in args {
            match arg.as_str() {
                "--allow-desktop-capture" => o.allow_capture = true,
                "--self-test" => o.self_test = true,
                "--compact" => o.compact = true,
                "--theme=dark" => o.dark = true,
                "--theme=light" => o.dark = false,
                "--review-mode" => o.review_mode = true,
                _ if arg.starts_with("--seconds=") => {
                    o.seconds = arg[10..].parse()?;
                    ensure((5..=1800).contains(&o.seconds), "seconds must be 5..1800")?;
                }
                _ if arg.starts_with("--snapshot-dir=") => {
                    let value = &arg[15..];
                    ensure(!value.is_empty(), "empty snapshot directory")?;
                    o.snapshots = Some(value.into());
                }
                _ => return Err(format!("Unknown option: {arg}").into()),
            }
        }
        ensure(o.self_test || o.allow_capture,
            "Live mode requires --allow-desktop-capture. It captures the primary SDR display locally on GPU and excludes THIS preview from system screen capture. Use --self-test for a no-capture GPU test.")?;
        ensure(!(o.self_test && o.allow_capture), "Do not combine self-test and live capture")?;
        ensure(!o.review_mode || (o.allow_capture && !o.self_test && o.snapshots.is_some()),
            "Review mode requires live capture consent and a new --snapshot-dir; it is not a self-test mode")?;
        Ok(o)
    }
}

pub fn run_preview() -> AppResult<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.iter().any(|a| a == "--help") {
        println!("GlassLivePreview --allow-desktop-capture [--compact] [--theme=light|dark] [--seconds=600] [--snapshot-dir=NEW_DIR] [--review-mode]\n\
                  Material: LENS_TRANSMISSION_1. Curved refraction, live transmission, adaptive ink, local glyph support, contact highlights and press/drag springs.\n\
                  Controls: left drag=move; left click=light/dark. Normal mode: middle click=local PNG, right click=close. No global hotkey.\n\
                  Review mode: middle/right mouse clicks do NOT save/close. Use review-preview.ps1 and one explicit SAVE for a same-frame light/dark pair. System close still works.\n\
                  Capture exclusion also hides this preview from many other screenshot/recording tools.\n\
                  --self-test runs the actual optical renderer on WARP without capturing or showing windows.");
        return Ok(());
    }
    let options = Options::parse(args.into_iter())?;
    #[cfg(windows)]
    {
        if options.self_test { unsafe { gpu::self_test(options.snapshots.as_deref()) } }
        else { unsafe { desktop::run(options) } }
    }
    #[cfg(not(windows))]
    {
        let _ = options;
        Err("Live and GPU self-test modes require Windows; portable unit tests are available via cargo test".into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test] fn capture_requires_opt_in() { assert!(parse(&[]).is_err()); }
    fn parse(a: &[&str]) -> AppResult<Options> { Options::parse(a.iter().map(|s| s.to_string())) }
    #[test] fn self_test_is_not_capture_consent() {
        let o = parse(&["--self-test"]).unwrap(); assert!(!o.allow_capture);
        assert!(parse(&["--self-test", "--allow-desktop-capture"]).is_err());
    }
    #[test] fn bounds_and_unknown_options() {
        assert!(parse(&["--allow-desktop-capture", "--seconds=0"]).is_err());
        assert!(parse(&["--allow-desktop-capture", "--unknown"]).is_err());
        assert_eq!(parse(&["--allow-desktop-capture", "--seconds=60"]).unwrap().seconds, 60);
        assert_eq!(parse(&["--self-test", "--snapshot-dir=test"]).unwrap().snapshots.unwrap(), std::path::PathBuf::from("test"));
    }
    #[test] fn review_requires_both_live_consent_and_snapshot_directory() {
        assert!(parse(&["--self-test","--review-mode","--snapshot-dir=test"]).is_err());
        assert!(parse(&["--allow-desktop-capture","--review-mode"]).is_err());
        assert!(parse(&["--allow-desktop-capture","--review-mode","--snapshot-dir=test"]).unwrap().review_mode);
    }
}

#[cfg(windows)]
mod voice_render_checks;
#[cfg(windows)]
pub fn voice_self_test() -> AppResult<()> { unsafe { voice_render_checks::verify(None) } }
