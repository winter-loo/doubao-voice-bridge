//! Standalone review; production uses this same library in-process.
fn main() {
    if let Err(error) = doubao_glass_live::run_preview() {
        eprintln!("[glass-live] ERROR: {error}");
        std::process::exit(1);
    }
}
