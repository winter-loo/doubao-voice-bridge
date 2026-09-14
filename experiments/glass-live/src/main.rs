//! Standalone custom-glass milestone. No GPUI, microphone, network or hotkeys.
//! Live capture requires an explicit flag. Self-test never captures the desktop.
#![allow(unsafe_op_in_unsafe_fn)]

mod png;
#[cfg(windows)]
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
    pub seconds: u64,
    pub snapshots: Option<std::path::PathBuf>,
}
impl Options {
    fn parse(args: impl Iterator<Item = String>) -> AppResult<Self> {
        let mut o = Self { allow_capture: false, self_test: false, compact: false,
            dark: false, seconds: 600, snapshots: None };
        for arg in args {
            match arg.as_str() {
                "--allow-desktop-capture" => o.allow_capture = true,
                "--self-test" => o.self_test = true,
                "--compact" => o.compact = true,
                "--theme=dark" => o.dark = true,
                "--theme=light" => o.dark = false,
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
        Ok(o)
    }
}

fn run() -> AppResult<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.iter().any(|a| a == "--help") {
        println!("GlassLivePreview --allow-desktop-capture [--compact] [--theme=light|dark] [--seconds=600] [--snapshot-dir=NEW_DIR]\n\
                  Controls: left drag=move; left click=light/dark; middle click=save a local crop (only when snapshot-dir was supplied); right click=close. No global hotkey.\n\
                  Capture exclusion also hides this preview from many other screenshot/recording tools.\n\
                  --self-test runs WARP shader/texture/readback checks without capturing or showing windows.");
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
fn main() {
    if let Err(e) = run() {
        eprintln!("[glass-live] ERROR: {e}");
        std::process::exit(1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn parse(a: &[&str]) -> AppResult<Options> { Options::parse(a.iter().map(|s| s.to_string())) }
    #[test] fn capture_requires_opt_in() { assert!(parse(&[]).is_err()); }
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
}
