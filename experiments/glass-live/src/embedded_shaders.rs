//! Build-time bytecode for the fixed, shipped shader sources. This is not a
//! disk cache: lookup requires the entire source AND entry AND target to match.
//! Synthetic/ablation/reference sources keep the original compiler path.
use super::*;
use windows::Win32::Graphics::Direct3D::Fxc::D3DCreateBlob;

struct EmbeddedShader {
    source: &'static str,
    entry: &'static [u8],
    target: &'static [u8],
    code: &'static [u8],
}
include!(concat!(env!("OUT_DIR"), "/embedded_shader_table.rs"));

fn lookup(source: &str, entry: &[u8], target: &[u8]) -> Option<&'static [u8]> {
    ENTRIES.iter().find(|s| s.entry == entry && s.target == target && s.source == source).map(|s| s.code)
}

pub(super) unsafe fn load(source: &str, entry: PCSTR, target: PCSTR) -> AppResult<Option<ID3DBlob>> {
    ensure(!entry.is_null() && !target.is_null(), "Missing HLSL entry or target")?;
    let Some(bytes) = lookup(source, entry.as_bytes(), target.as_bytes()) else { return Ok(None); };
    let blob = D3DCreateBlob(bytes.len())?;
    std::ptr::copy_nonoverlapping(bytes.as_ptr(), blob.GetBufferPointer().cast::<u8>(), bytes.len());
    #[cfg(test)]
    COUNTS.with(|c| { let (hits, misses) = c.get(); c.set((hits + 1, misses)); });
    Ok(Some(blob))
}

#[cfg(test)]
thread_local! { static COUNTS: std::cell::Cell<(u64, u64)> = const { std::cell::Cell::new((0, 0)) }; }
#[cfg(test)]
pub(super) fn runtime_compile() {
    COUNTS.with(|c| { let (hits, misses) = c.get(); c.set((hits, misses + 1)); });
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::ffi::CString;

    #[test]
    fn embedded_bytecode_matches_original_runtime_compiler() { unsafe {
        assert_eq!(ENTRIES.len(), 9);
        for shader in ENTRIES {
            let entry = CString::new(shader.entry).unwrap();
            let target = CString::new(shader.target).unwrap();
            let entry = PCSTR(entry.as_ptr().cast());
            let target = PCSTR(target.as_ptr().cast());
            let original = super::super::compile_source_runtime(shader.source, entry, target).unwrap();
            assert_eq!(shader.code, super::super::blob_bytes(&original),
                "Build-time compiler output differs from the original runtime source/entry/flags");
            let embedded = load(shader.source, entry, target).unwrap().unwrap();
            assert_eq!(shader.code, super::super::blob_bytes(&embedded));
        }
        eprintln!("[embedded-shaders] PASS; 9 exact build-time/runtime bytecode comparisons; same HLSL and flags; no windows/capture/audio");
    } }

    #[test]
    fn fresh_voice_constructors_never_compile_hlsl_at_runtime() { unsafe {
        use crate::voice_model::Phase;
        let mask = crate::voice_window::content_mask(162, 39, 1.5, Phase::Activating).unwrap();
        for _ in 0..2 {
            let (device, context) = super::super::create_device(None).unwrap();
            let before = COUNTS.with(|c| c.get());
            let pipe = crate::gpu::AdaptivePipeline::new_voice(device, context, 162, 39, &mask).unwrap();
            let after = COUNTS.with(|c| c.get());
            assert_eq!((after.0 - before.0, after.1 - before.1), (9, 0),
                "Live constructor fell back to runtime HLSL compilation; update the build-time entry set");
            drop(pipe);
        }
        eprintln!("[embedded-shaders] PASS; 2 fresh voice constructors; 18 embedded loads; 0 runtime HLSL compilations; WARP, not a live latency measurement");
    } }

    #[test]
    fn source_entry_and_target_are_all_part_of_the_key() {
        for shader in ENTRIES {
            assert!(lookup(shader.source, shader.entry, shader.target).is_some());
            assert!(lookup(&format!("{}\n// different fixture", shader.source), shader.entry, shader.target).is_none());
            assert!(lookup(shader.source, b"unknown_entry", shader.target).is_none());
            assert!(lookup(shader.source, shader.entry, b"ps_4_0").is_none());
        }
    }
}
