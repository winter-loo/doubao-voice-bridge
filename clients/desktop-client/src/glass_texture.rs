//! GPUI-side installation of asynchronously baked glass pixels. This application
//! has one overlay window; its current image ladder is the only ladder retained
//! here. The worker knows nothing about GPUI, Window or sprite-atlas lifetimes.

use std::sync::{Arc, Mutex, OnceLock};
use gpui::{RenderImage, Window};
use image::{Frame, RgbaImage};
use super::{GLASS_MIX_STEPS, glass_bake::LatestBake, liquid_glass};

type Frames = Vec<Vec<u8>>;
type Ladder = Vec<Frames>;

#[derive(Clone, Copy, PartialEq, Eq)]
struct Key { width: u32, height: u32, tint: u32 }

struct BaseFrames {
    size: (u32, u32),
    light: Frames,
    dark: Frames,
}

struct Cache {
    worker: LatestBake<Key, Ladder>,
    current: Option<(Key, Vec<Arc<RenderImage>>)>,
}

impl Cache {
    fn new() -> std::io::Result<Self> {
        // Bound the base cache to one size. Moving between monitors must not
        // retain every old DPI's frames for the lifetime of the process.
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
            let light = liquid_glass::tint_frames(&base.light, tint);
            let dark = liquid_glass::tint_frames(&base.dark, tint);
            let mut ladder = Vec::with_capacity(GLASS_MIX_STEPS as usize + 1);
            // Each endpoint is moved into the ladder, not cloned. All expensive
            // per-pixel grading/blending is done on this single worker.
            ladder.push(light);
            for step in 1..GLASS_MIX_STEPS {
                ladder.push(liquid_glass::blend_frames(
                    &ladder[0], &dark, step as f32 / GLASS_MIX_STEPS as f32,
                ));
            }
            ladder.push(dark);
            ladder
        })?;
        Ok(Self { worker, current: None })
    }
}

pub fn glass_texture(
    window: &mut Window,
    width: u32,
    height: u32,
    tint: u32,
    level: u32,
) -> Option<Arc<RenderImage>> {
    static CACHE: OnceLock<Mutex<Option<Cache>>> = OnceLock::new();
    let mut slot = CACHE.get_or_init(|| Mutex::new(match Cache::new() {
        Ok(cache) => Some(cache),
        Err(error) => {
            eprintln!("[glass] could not start bake worker: {error}");
            None
        }
    })).lock().unwrap_or_else(|p| p.into_inner());
    let cache = slot.as_mut()?;
    let key = Key { width, height, tint };
    cache.worker.request(key);
    if let Some(pixels) = cache.worker.take_ready(key) {
        let images = pixels.into_iter().map(|frames| {
            let frames = frames.into_iter().map(|pixels| {
                Frame::new(RgbaImage::from_raw(width, height, pixels)
                    .expect("glass frame is width * height * 4 bytes"))
            }).collect::<Vec<_>>();
            Arc::new(RenderImage::new(frames))
        }).collect();
        if let Some((_, old)) = cache.current.replace((key, images)) {
            for image in old { let _ = window.drop_image(image); }
        }
    }
    // While a new tint is baking, keep the old glass at the requested light/dark
    // level. On a cold start (or a size change) return None for the flat fallback.
    let (current, ladder) = cache.current.as_ref()?;
    if current.width != width || current.height != height { return None; }
    ladder.get(level.min(GLASS_MIX_STEPS) as usize).cloned()
}
