//! UI installation of asynchronously baked glass pixels. Native blur and the
//! solid content-protection fallback share this path with GlassPreview.

use std::sync::{Arc, Mutex, OnceLock};
use gpui::{RenderImage, Window};
use image::{Frame, RgbaImage};
use super::{GLASS_MIX_STEPS, glass_bake::LatestBake, liquid_glass};
#[path = "glass_policy.rs"]
pub mod policy;
#[path = "glass_native.rs"]
mod native;

type Frames = Vec<Vec<u8>>;
type Ladder = Vec<Frames>;

#[derive(Clone, Copy, PartialEq, Eq)]
struct Key { width: u32, height: u32, tint: u32, backend: policy::Backend }
struct BaseFrames { size: (u32, u32), light: Frames, dark: Frames }
struct Cache {
    worker: LatestBake<Key, Ladder>,
    current: Option<(Key, Vec<Arc<RenderImage>>)>,
}
impl Cache {
    fn new() -> std::io::Result<Self> {
        let mut bases: Option<BaseFrames> = None;
        let worker = LatestBake::new(move |key: Key| {
            let size = (key.width, key.height);
            if bases.as_ref().is_none_or(|base| base.size != size) {
                bases = Some(BaseFrames {
                    size,
                    light: liquid_glass::CapsuleGlass::new(key.width, key.height, false).render_frames(),
                    dark: liquid_glass::CapsuleGlass::new(key.width, key.height, true).render_frames(),
                });
            }
            let base = bases.as_ref().expect("base frames were generated");
            let tint = liquid_glass::Tint::from_quantized(key.tint);
            let mut light = liquid_glass::tint_frames(&base.light, tint);
            let mut dark = liquid_glass::tint_frames(&base.dark, tint);
            policy::protect_content(&mut light, key.width, key.height, false, key.backend).expect("valid light frames");
            policy::protect_content(&mut dark, key.width, key.height, true, key.backend).expect("valid dark frames");
            let mut ladder = Vec::with_capacity(GLASS_MIX_STEPS as usize + 1);
            ladder.push(light);
            for step in 1..GLASS_MIX_STEPS {
                ladder.push(liquid_glass::blend_frames(&ladder[0], &dark, step as f32 / GLASS_MIX_STEPS as f32));
            }
            ladder.push(dark);
            ladder
        })?;
        Ok(Self { worker, current: None })
    }
}

pub fn glass_texture(window: &mut Window, width: u32, height: u32, tint: u32, level: u32) -> Option<Arc<RenderImage>> {
    let backend = native::prepare(window);
    static CACHE: OnceLock<Mutex<Option<Cache>>> = OnceLock::new();
    let mut slot = CACHE.get_or_init(|| Mutex::new(match Cache::new() {
        Ok(cache) => Some(cache),
        Err(error) => { eprintln!("[glass] could not start bake worker: {error}"); None }
    })).lock().unwrap_or_else(|p| p.into_inner());
    let cache = slot.as_mut()?;
    let key = Key { width, height, tint, backend };
    cache.worker.request(key);
    if let Some(pixels) = cache.worker.take_ready(key) {
        let images = pixels.into_iter().map(|frames| {
            let frames = frames.into_iter().map(|pixels| {
                Frame::new(RgbaImage::from_raw(width, height, pixels).expect("valid glass frame"))
            }).collect::<Vec<_>>();
            Arc::new(RenderImage::new(frames))
        }).collect();
        if let Some((_, old)) = cache.current.replace((key, images)) {
            for image in old { let _ = window.drop_image(image); }
        }
    }
    let (current, ladder) = cache.current.as_ref()?;
    if current.width != width || current.height != height || current.backend != backend { return None; }
    ladder.get(level.min(GLASS_MIX_STEPS) as usize).cloned()
}
