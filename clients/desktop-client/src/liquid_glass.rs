//! Procedural glass-like shading for the voice overlay capsule.
//!
//! The distance field, bevel and displaced sampling follow the geometry of a lens,
//! but `sample_interior` reads our own gradient, NOT pixels behind the window. Real
//! desktop refraction needs a separate live backdrop source and render path. Only
//! geometry and surface shading, not a live refracted backdrop, can be baked here.
//!
//! Frames use straight-alpha BGRA for GPUI's RenderImage. This module has no GPUI or
//! window-system dependency and can be tested with `rustc --test liquid_glass.rs`.

pub const SHEEN_FRAMES: usize = 24;

/// Keep the bevel narrow; the middle is a flat, continuous slab, not an inset trough.
const BEVEL_RATIO: f32 = 0.30;
const DISPLACEMENT_RATIO: f32 = 0.55;
const BEVEL_SHARPNESS: f32 = 3.9;
const LIGHT: (f32, f32) = (-0.32, -1.0);
const THICKNESS_GAIN: f32 = 0.25;

/// Distances below are at the overlay's 26 logical-pixel reference height. Scaling
/// them with the lens preserves the same material at 100%, 150% and 200% DPI. The
/// silhouette's antialiasing coverage still spans one DEVICE pixel.
const REFERENCE_HEIGHT: f32 = 26.0;
const OUTLINE_INSET: f32 = 0.45;
const OUTLINE_WIDTH: f32 = 0.50;
const HIGHLIGHT_INSET: f32 = 1.55;
const HIGHLIGHT_WIDTH: f32 = 0.60;
const SHEEN_WIDTH: f32 = 0.17;

/// Straight-alpha color; byte conventions match GPUI's rgba() literals.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Rgba {
    pub r: f32,
    pub g: f32,
    pub b: f32,
    pub a: f32,
}

impl Rgba {
    const TRANSPARENT: Self = Self { r: 0.0, g: 0.0, b: 0.0, a: 0.0 };

    const fn hex(value: u32) -> Self {
        Self {
            r: ((value >> 24) & 0xff) as f32 / 255.0,
            g: ((value >> 16) & 0xff) as f32 / 255.0,
            b: ((value >> 8) & 0xff) as f32 / 255.0,
            a: (value & 0xff) as f32 / 255.0,
        }
    }

    fn with_alpha(self, alpha: f32) -> Self {
        Self { a: alpha.clamp(0.0, 1.0), ..self }
    }

    fn lerp(self, other: Self, t: f32) -> Self {
        let t = t.clamp(0.0, 1.0);
        Self {
            r: self.r + (other.r - self.r) * t,
            g: self.g + (other.g - self.g) * t,
            b: self.b + (other.b - self.b) * t,
            a: self.a + (other.a - self.a) * t,
        }
    }

    fn over(self, under: Self) -> Self {
        let out_a = self.a + under.a * (1.0 - self.a);
        if out_a <= f32::EPSILON { return Self::TRANSPARENT; }
        let blend = |top: f32, bottom: f32| {
            (top * self.a + bottom * under.a * (1.0 - self.a)) / out_a
        };
        Self {
            r: blend(self.r, under.r),
            g: blend(self.g, under.g),
            b: blend(self.b, under.b),
            a: out_a,
        }
    }
}

struct Palette {
    tint_top: Rgba,
    tint_bottom: Rgba,
    inner_wall: Rgba,
    /// Artistic edge reflections, not spectral sampling of the desktop.
    dispersion_cool: Rgba,
    dispersion_warm: Rgba,
    /// A restrained contrast line, independent of specular lighting.
    outline: Rgba,
    key_light: Rgba,
    bounce_light: Rgba,
    sheen: Rgba,
}

const DARK_PALETTE: Palette = Palette {
    tint_top: Rgba::hex(0x5f7f9660),
    tint_bottom: Rgba::hex(0x07121f8a),
    inner_wall: Rgba::hex(0x040c1618),
    dispersion_cool: Rgba::hex(0x8ee9ff23),
    dispersion_warm: Rgba::hex(0xffc8f11a),
    outline: Rgba::hex(0x0e1c2b28),
    key_light: Rgba::hex(0xffffffec),
    bounce_light: Rgba::hex(0xdbeeff58),
    sheen: Rgba::hex(0xdff2ff1c),
};

/// White backgrounds need a little contour contrast, not an opaque body. The dark
/// contour is painted BEFORE the inward, neutral highlight so it cannot erase it.
const LIGHT_PALETTE: Palette = Palette {
    tint_top: Rgba::hex(0xeaf2fa46),
    tint_bottom: Rgba::hex(0xbfd0e246),
    inner_wall: Rgba::hex(0x3a587218),
    dispersion_cool: Rgba::hex(0x3dbcff1c),
    dispersion_warm: Rgba::hex(0xff86cf16),
    outline: Rgba::hex(0x4a648448),
    key_light: Rgba::hex(0xffffffe6),
    bounce_light: Rgba::hex(0xeaf4ff68),
    sheen: Rgba::hex(0xffffff30),
};

pub struct CapsuleGlass {
    width: f32,
    height: f32,
    radius: f32,
    bevel: f32,
    displacement: f32,
    palette: &'static Palette,
}

impl CapsuleGlass {
    /// Dimensions are device pixels. The caller applies the window scale factor.
    pub fn new(width: u32, height: u32, dark: bool) -> Self {
        let width = width.max(1) as f32;
        let height = height.max(1) as f32;
        let radius = width.min(height) / 2.0;
        let bevel = (radius * BEVEL_RATIO).max(1.0);
        Self {
            width, height, radius, bevel,
            displacement: bevel * DISPLACEMENT_RATIO,
            palette: if dark { &DARK_PALETTE } else { &LIGHT_PALETTE },
        }
    }

    /// Signed distance and outward unit normal of the rounded rectangle.
    fn field(&self, x: f32, y: f32) -> (f32, f32, f32) {
        let qx = x - self.width / 2.0;
        let qy = y - self.height / 2.0;
        let ax = qx.abs() - (self.width / 2.0 - self.radius);
        let ay = qy.abs() - (self.height / 2.0 - self.radius);
        let sx = if qx < 0.0 { -1.0 } else { 1.0 };
        let sy = if qy < 0.0 { -1.0 } else { 1.0 };
        if ax > 0.0 && ay > 0.0 {
            let len = (ax * ax + ay * ay).sqrt();
            if len <= f32::EPSILON { return (-self.radius, sx, 0.0); }
            (len - self.radius, sx * ax / len, sy * ay / len)
        } else if ax > ay {
            (ax - self.radius, sx, 0.0)
        } else {
            (ay - self.radius, 0.0, sy)
        }
    }

    /// Synthetic body color, not a captured desktop texture. Integrating a live
    /// backdrop also requires changing invalidation, caching and GPU composition.
    fn sample_interior(&self, _x: f32, y: f32) -> Rgba {
        self.palette.tint_top.lerp(
            self.palette.tint_bottom, (y / self.height).clamp(0.0, 1.0),
        )
    }

    pub fn shade(&self, x: f32, y: f32, phase: f32) -> Rgba {
        let (distance, nx, ny) = self.field(x, y);
        let coverage = (0.5 - distance).clamp(0.0, 1.0);
        if coverage <= 0.0 { return Rgba::TRANSPARENT; }
        let t = (1.0 + distance / self.bevel).clamp(0.0, 1.0);
        let rim = bevel_profile(t);
        let offset = rim * self.displacement;
        let light_len = (LIGHT.0 * LIGHT.0 + LIGHT.1 * LIGHT.1).sqrt();
        let facing = ((nx * LIGHT.0 + ny * LIGHT.1) / light_len).max(0.0);
        let scale = self.height / REFERENCE_HEIGHT;

        let mut color = self.sample_interior(x - nx * offset, y - ny * offset);
        color = color.with_alpha(color.a * (1.0 + THICKNESS_GAIN * rim));

        // Clamping t to zero used to leave exp(-(0.34/0.26)^2) of this ring
        // EVERYWHERE in the interior. The gate makes both value and slope vanish
        // at the flat boundary, including where the SDF normal changes direction.
        let wall = inner_wall_band(t) * (1.0 - 0.6 * facing);
        color = self.palette.inner_wall
            .with_alpha(self.palette.inner_wall.a * wall).over(color);

        let dispersion = self.palette.dispersion_cool
            .with_alpha(self.palette.dispersion_cool.a * rim * (-nx).max(0.0))
            .over(self.palette.dispersion_warm
                .with_alpha(self.palette.dispersion_warm.a * rim * nx.max(0.0)));
        color = dispersion.over(color);

        // Visibility is not illumination: a dark stroke must not become strongest
        // at the key light or be composited over the specular highlight.
        let outline = gaussian(distance + OUTLINE_INSET * scale, OUTLINE_WIDTH * scale)
            * (0.65 + 0.35 * (1.0 - facing));
        color = self.palette.outline
            .with_alpha(self.palette.outline.a * outline).over(color);

        // Neutral highlights sit inward from the contrast line. Every lighting
        // term is gated off in the flat interior, independently of image size.
        let highlight = gaussian(
            distance + HIGHLIGHT_INSET * scale, HIGHLIGHT_WIDTH * scale,
        ) * smoothstep((t / 0.18).clamp(0.0, 1.0));
        let bounce = ny.max(0.0).powf(3.0) * (0.55 * rim + 0.45 * highlight);
        color = self.palette.bounce_light
            .with_alpha(self.palette.bounce_light.a * bounce).over(color);
        let key = facing.powf(2.2) * (0.25 * rim + 0.75 * highlight);
        color = self.palette.key_light
            .with_alpha(self.palette.key_light.a * key).over(color);

        let travel = -0.3 + 1.6 * phase;
        let along = (x + y * 0.45) / self.width;
        let sheen = gaussian(along - travel, SHEEN_WIDTH) * (0.3 + 0.7 * rim);
        color = self.palette.sheen.with_alpha(self.palette.sheen.a * sheen).over(color);
        color.with_alpha(color.a * coverage)
    }

    pub fn render_bgra(&self, frame: usize) -> Vec<u8> {
        let width = self.width as usize;
        let height = self.height as usize;
        let phase = (frame % SHEEN_FRAMES) as f32 / SHEEN_FRAMES as f32;
        let mut pixels = Vec::with_capacity(width * height * 4);
        for y in 0..height {
            for x in 0..width {
                let c = self.shade(x as f32 + 0.5, y as f32 + 0.5, phase);
                pixels.extend_from_slice(&[to_byte(c.b), to_byte(c.g), to_byte(c.r), to_byte(c.a)]);
            }
        }
        pixels
    }

    pub fn render_frames(&self) -> Vec<Vec<u8>> {
        (0..SHEEN_FRAMES).map(|frame| self.render_bgra(frame)).collect()
    }
}

fn smoothstep(t: f32) -> f32 { t * t * (3.0 - 2.0 * t) }

fn inner_wall_band(t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    smoothstep((t / 0.18).clamp(0.0, 1.0)) * gaussian(t - 0.34, 0.26)
}

#[cfg_attr(not(target_os = "windows"), allow(dead_code))]
const MAX_CAST: f32 = 0.45;
#[cfg_attr(not(target_os = "windows"), allow(dead_code))]
const CAST_STRENGTH: f32 = 0.35;

/// A restrained relative color cast. Normalizing the measured color makes this
/// exposure-invariant; it does not guarantee identical luminance after grading.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Tint { pub r: f32, pub g: f32, pub b: f32 }

#[cfg_attr(not(target_os = "windows"), allow(dead_code))]
impl Tint {
    pub const NEUTRAL: Self = Self { r: 1.0, g: 1.0, b: 1.0 };
    pub const NEUTRAL_KEY: u32 = 0x888;

    pub fn from_background(red: u8, green: u8, blue: u8) -> Self {
        let (r, g, b) = (f32::from(red), f32::from(green), f32::from(blue));
        let luminance = 0.2126 * r + 0.7152 * g + 0.0722 * b;
        if luminance <= 1.0 { return Self::NEUTRAL; }
        let channel = |c: f32| {
            let ratio = (c / luminance).clamp(1.0 - MAX_CAST, 1.0 + MAX_CAST);
            1.0 + (ratio - 1.0) * CAST_STRENGTH
        };
        Self { r: channel(r), g: channel(g), b: channel(b) }
    }

    pub fn quantized(self) -> u32 {
        let step = |v: f32| (((v - 1.0) * 16.0).round() as i32 + 8).clamp(0, 15) as u32;
        (step(self.r) << 8) | (step(self.g) << 4) | step(self.b)
    }

    /// A Schmitt band of 0.15 bucket widths around each rounding boundary. A small
    /// oscillation around one boundary cannot alternate cache keys every sample.
    pub fn quantized_near(self, previous: u32) -> u32 {
        let channel = |v: f32, shift: u32| {
            let old = ((previous >> shift) & 0xf) as f32;
            let measured = (v - 1.0) * 16.0 + 8.0;
            if (measured - old).abs() <= 0.65 {
                old as u32
            } else {
                measured.round().clamp(0.0, 15.0) as u32
            }
        };
        (channel(self.r, 8) << 8) | (channel(self.g, 4) << 4) | channel(self.b, 0)
    }

    pub fn from_quantized(key: u32) -> Self {
        let value = |shift: u32| 1.0 + (((key >> shift) & 0xf) as f32 - 8.0) / 16.0;
        Self { r: value(8), g: value(4), b: value(0) }
    }
}

pub fn tint_frames(frames: &[Vec<u8>], tint: Tint) -> Vec<Vec<u8>> {
    frames.iter().map(|frame| {
        frame.chunks_exact(4).flat_map(|p| {
            let scale = |c: u8, m: f32| (f32::from(c) * m).round().clamp(0.0, 255.0) as u8;
            [scale(p[0], tint.b), scale(p[1], tint.g), scale(p[2], tint.r), p[3]]
        }).collect()
    }).collect()
}

const BLEND_ONE: u32 = 256;

/// Interpolate straight-alpha bytes, including alpha (not source-over). This
/// retains the existing transition policy; it is not premultiplied color blending.
pub fn blend_frames(start: &[Vec<u8>], end: &[Vec<u8>], mix: f32) -> Vec<Vec<u8>> {
    assert_eq!(start.len(), end.len(), "frame sets differ in length");
    let mix = (mix.clamp(0.0, 1.0) * BLEND_ONE as f32).round() as u32;
    start.iter().zip(end).map(|(from, to)| {
        assert_eq!(from.len(), to.len(), "frames differ in size");
        from.iter().zip(to).map(|(&a, &b)| {
            ((u32::from(a) * (BLEND_ONE - mix) + u32::from(b) * mix + BLEND_ONE / 2) >> 8) as u8
        }).collect()
    }).collect()
}

fn bevel_profile(t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    let dome = (1.0 - t.powf(BEVEL_SHARPNESS)).max(1.0e-4);
    let slope = t.powf(BEVEL_SHARPNESS - 1.0) * dome.powf(1.0 / BEVEL_SHARPNESS - 1.0);
    slope / (1.0 + slope)
}
fn gaussian(offset: f32, width: f32) -> f32 {
    let n = offset / width;
    (-n * n).exp()
}
fn to_byte(value: f32) -> u8 { (value.clamp(0.0, 1.0) * 255.0).round() as u8 }

#[cfg(test)]
mod tests {
    use super::*;
    const WIDTH: u32 = 216;
    const HEIGHT: u32 = 52;
    fn glass() -> CapsuleGlass { CapsuleGlass::new(WIDTH, HEIGHT, true) }
    fn luminance(c: Rgba) -> f32 { c.r * 0.2126 + c.g * 0.7152 + c.b * 0.0722 }
    fn over_white(c: Rgba) -> f32 { luminance(c) * c.a + 1.0 - c.a }

    #[test]
    fn distance_field_matches_capsule_geometry() {
        let g = glass();
        assert!((g.field(108.0, 26.0).0 + g.radius).abs() < 0.01);
        assert_eq!(g.field(108.0, 4.0), (-4.0, 0.0, -1.0));
        assert_eq!(g.field(0.0, 26.0), (0.0, -1.0, 0.0));
    }
    #[test]
    fn distance_field_is_positive_outside_the_round_cap() {
        assert!(glass().field(0.5, 0.5).0 > 0.0);
    }
    #[test]
    fn bevel_profile_runs_from_flat_to_full_bend() {
        assert_eq!(bevel_profile(0.0), 0.0);
        assert!(bevel_profile(1.0) > 0.99);
        let mut previous = 0.0;
        for step in 0..=100 {
            let value = bevel_profile(step as f32 / 100.0);
            assert!(value >= previous);
            previous = value;
        }
        assert!(bevel_profile(0.5) < 0.25);
    }
    #[test]
    fn inner_wall_is_zero_throughout_the_flat_interior() {
        for t in [-20.0, -1.0, -0.01, 0.0] { assert_eq!(inner_wall_band(t), 0.0); }
        assert!(inner_wall_band(0.001) < 0.0001);
        assert!(inner_wall_band(0.34) > 0.99);
    }
    #[test]
    fn the_flat_slab_has_no_normal_driven_midline_seam() {
        // The original wall had a nonzero tail at t=0; flipping ny across the
        // midline changed that tail abruptly. Remove the smooth body and sheen
        // analytically, then check that nothing else contributes in the flat slab.
        for dark in [false, true] {
            let g = CapsuleGlass::new(WIDTH, HEIGHT, dark);
            for y in [13.0, 25.99, 26.0, 26.01, 39.0] {
                let x = 108.0;
                let phase = 0.2;
                let strength = gaussian((x + y * 0.45) / g.width - (-0.3 + 1.6 * phase), SHEEN_WIDTH) * 0.3;
                let expected = g.palette.sheen.with_alpha(g.palette.sheen.a * strength)
                    .over(g.sample_interior(x, y));
                let actual = g.shade(x, y, phase);
                for (a, b) in [(actual.r, expected.r), (actual.g, expected.g),
                    (actual.b, expected.b), (actual.a, expected.a)] {
                    assert!((a - b).abs() < 1.0e-5, "y={y}: {actual:?} vs {expected:?}");
                }
            }
        }
    }
    #[test]
    fn corners_are_transparent_and_the_slab_is_not() {
        let g = glass();
        assert_eq!(g.shade(0.5, 0.5, 0.0).a, 0.0);
        assert!(g.shade(108.0, 26.0, 0.0).a > 0.1);
    }
    #[test]
    fn rim_is_brighter_than_the_slab_interior() {
        let g = glass();
        let center = g.shade(108.0, 26.0, 0.5);
        let top = g.shade(108.0, HIGHLIGHT_INSET * 2.0, 0.5);
        assert!(luminance(top) > luminance(center) + 0.2);
        assert!(top.a > center.a);
        assert!(luminance(top) * top.a > 0.5);
    }
    #[test]
    fn the_light_capsule_separates_from_a_white_page_without_a_heavy_top_stroke() {
        let g = CapsuleGlass::new(WIDTH, HEIGHT, false);
        // Sample the full contour rather than requiring the lit top pixel to be
        // dark. Both endpoints of each cap and both straight edges contribute.
        let r = g.radius - OUTLINE_INSET * 2.0;
        let mut contrast = 0.0;
        for step in 0..64 {
            let a = step as f32 * core::f32::consts::TAU / 64.0;
            let cx = if a.cos() < 0.0 { g.radius } else { g.width - g.radius };
            contrast += 1.0 - over_white(g.shade(cx + r * a.cos(), g.radius + r * a.sin(), 0.0));
        }
        assert!(contrast / 64.0 > 0.08, "outline disappeared: {}", contrast / 64.0);
        let outline = over_white(g.shade(108.0, OUTLINE_INSET * 2.0, 0.0));
        let highlight = over_white(g.shade(108.0, HIGHLIGHT_INSET * 2.0, 0.0));
        assert!(highlight > outline + 0.04, "highlight {highlight}, outline {outline}");
        assert!(highlight > 0.93, "dark outline erased the highlight: {highlight}");
    }
    #[test]
    fn the_capsule_stays_see_through() {
        let survives = |dark| 1.0 - CapsuleGlass::new(WIDTH, HEIGHT, dark).shade(108.0, 26.0, 0.5).a;
        assert!(survives(false) > 0.6);
        assert!(survives(true) > 0.45);
        assert!(survives(false) > survives(true));
    }
    #[test]
    fn highlight_wraps_the_round_cap() {
        let g = glass();
        let r = g.radius - HIGHLIGHT_INSET * 2.0;
        let lit = g.shade(g.radius - r * 0.6, g.radius - r * 0.8, 0.0);
        let unlit = g.shade(g.width - g.radius + r * 0.6, g.radius - r * 0.8, 0.0);
        assert!(luminance(lit) * lit.a > luminance(unlit) * unlit.a);
    }
    #[test]
    fn edge_reflections_are_restrained() {
        for p in [&LIGHT_PALETTE, &DARK_PALETTE] {
            assert!(p.dispersion_cool.a < 0.15);
            assert!(p.dispersion_warm.a < 0.15);
        }
    }
    #[test]
    fn dispersion_splits_across_the_two_flanks() {
        let g = glass();
        let left = g.shade(1.0, 26.0, 0.5);
        let right = g.shade(215.0, 26.0, 0.5);
        assert!(left.b - left.r > right.b - right.r);
    }
    #[test]
    fn shading_scales_with_device_pixel_ratio() {
        for dark in [false, true] {
            let base = CapsuleGlass::new(108, 26, dark);
            for scale in [1.25, 1.5, 2.0] {
                let height = (26.0_f32 * scale).round() as u32;
                let g = CapsuleGlass::new((108.0_f32 * scale).round() as u32, height, dark);
                let actual_scale = height as f32 / 26.0;
                for y in [OUTLINE_INSET, HIGHLIGHT_INSET, 6.0, 13.0] {
                    // Turn off the moving sheen by putting it far outside the lens.
                    let a = base.shade(54.0, y, 10.0);
                    let b = g.shade(g.width / 2.0, y * actual_scale, 10.0);
                    // AA is intentionally device-based, so compare unassociated RGB.
                    for (a, b) in [(a.r, b.r), (a.g, b.g), (a.b, b.b)] {
                        assert!((a - b).abs() < 0.001, "scale={scale}, y={y}: {a} vs {b}");
                    }
                }
            }
        }
    }
    #[test]
    fn sheen_moves_between_phases() {
        let g = glass();
        assert!((g.shade(54.0, 15.6, 0.0).a - g.shade(54.0, 15.6, 0.5).a).abs() > 1.0e-4);
    }
    #[test]
    fn rendered_frames_have_the_expected_shape() {
        let frames = CapsuleGlass::new(40, 20, false).render_frames();
        assert_eq!(frames.len(), SHEEN_FRAMES);
        for f in frames { assert_eq!(f.len(), 40 * 20 * 4); assert_eq!(f[3], 0); }
    }
    #[test]
    fn over_composites_like_source_over() {
        let opaque = Rgba::hex(0xff0000ff);
        assert_eq!(opaque.over(Rgba::TRANSPARENT), opaque);
        assert_eq!(Rgba::TRANSPARENT.over(opaque), opaque);
        let c = Rgba::hex(0xffffff80).over(Rgba::hex(0x000000ff));
        assert!((c.a - 1.0).abs() < 1.0e-6);
        assert!((c.r - 128.0 / 255.0).abs() < 0.01);
    }
    #[test]
    fn blending_returns_each_end_untouched() {
        let a = CapsuleGlass::new(24, 12, false).render_frames();
        let b = CapsuleGlass::new(24, 12, true).render_frames();
        assert_eq!(blend_frames(&a, &b, 0.0), a);
        assert_eq!(blend_frames(&a, &b, 1.0), b);
    }
    #[test]
    fn blending_stays_between_the_two_ends() {
        let a = CapsuleGlass::new(24, 12, false).render_frames();
        let b = CapsuleGlass::new(24, 12, true).render_frames();
        for (mixed, (a, b)) in blend_frames(&a, &b, 0.5).iter().zip(a.iter().zip(&b)) {
            for ((&m, &a), &b) in mixed.iter().zip(a).zip(b) { assert!((a.min(b)..=a.max(b)).contains(&m)); }
        }
    }
    #[test]
    fn alpha_moves_linearly_so_the_capsule_never_thickens() {
        let a = CapsuleGlass::new(24, 12, false).render_frames();
        let b = CapsuleGlass::new(24, 12, true).render_frames();
        for (m, (a, b)) in blend_frames(&a, &b, 0.5).iter().zip(a.iter().zip(&b)) {
            for i in (3..m.len()).step_by(4) {
                assert!((f32::from(m[i]) - (f32::from(a[i]) + f32::from(b[i])) / 2.0).abs() <= 0.5);
            }
        }
    }
    #[test]
    fn a_cast_is_invariant_under_exposure_scaling() {
        let a = Tint::from_background(24, 40, 96);
        let b = Tint::from_background(48, 80, 192);
        for (a, b) in [(a.r, b.r), (a.g, b.g), (a.b, b.b)] { assert!((a - b).abs() < 1.0e-4); }
        assert!(a.b > 1.0 && a.r < 1.0);
    }
    #[test]
    fn a_paler_colour_casts_more_weakly_than_a_saturated_one() {
        let saturated = Tint::from_background(24, 40, 96);
        let pale = Tint::from_background(160, 190, 240);
        assert!(pale.b > 1.0 && pale.b < saturated.b);
    }
    #[test]
    fn the_spelled_out_neutral_key_matches_the_computed_one() {
        assert_eq!(Tint::NEUTRAL.quantized(), Tint::NEUTRAL_KEY);
        assert_eq!(Tint::from_quantized(Tint::NEUTRAL_KEY), Tint::NEUTRAL);
    }
    #[test]
    fn a_grey_desktop_produces_no_cast() {
        for level in [0, 64, 128, 200, 255] {
            assert_eq!(Tint::from_background(level, level, level).quantized(), Tint::NEUTRAL_KEY);
        }
    }
    #[test]
    fn a_saturated_desktop_is_clamped_rather_than_followed() {
        let c = Tint::from_background(255, 0, 0);
        assert!(c.r <= 1.0 + MAX_CAST * CAST_STRENGTH + 1.0e-4);
        assert!(c.g >= 1.0 - MAX_CAST * CAST_STRENGTH - 1.0e-4);
    }
    #[test]
    fn quantising_a_cast_round_trips_through_its_bucket() {
        for (r, g, b) in [(20, 30, 90), (200, 190, 160), (128, 128, 128), (10, 90, 40)] {
            let c = Tint::from_background(r, g, b);
            let canonical = Tint::from_quantized(c.quantized());
            assert_eq!(canonical.quantized(), c.quantized());
            assert!((canonical.r - c.r).abs() < 0.05 && (canonical.b - c.b).abs() < 0.05);
        }
    }
    #[test]
    fn tint_quantization_does_not_chatter_at_a_bucket_boundary() {
        let mut key = Tint::NEUTRAL_KEY;
        for step in 0..100 {
            let r = 1.0 + (if step % 2 == 0 { 0.49 } else { 0.51 }) / 16.0;
            key = Tint { r, ..Tint::NEUTRAL }.quantized_near(key);
            assert_eq!(key, Tint::NEUTRAL_KEY);
        }
        key = Tint { r: 1.0 + 0.70 / 16.0, ..Tint::NEUTRAL }.quantized_near(key);
        assert_eq!(key, 0x988);
        key = Tint { r: 1.0 + 0.49 / 16.0, ..Tint::NEUTRAL }.quantized_near(key);
        assert_eq!(key, 0x988);
        assert_eq!(Tint::NEUTRAL.quantized_near(key), Tint::NEUTRAL_KEY);
    }
    #[test]
    fn tinting_leaves_alpha_untouched() {
        let frames = CapsuleGlass::new(24, 12, false).render_frames();
        let tinted = tint_frames(&frames, Tint::from_background(30, 60, 200));
        for (a, b) in frames.iter().zip(tinted) {
            for i in (3..a.len()).step_by(4) { assert_eq!(a[i], b[i]); }
        }
    }
    #[test]
    fn a_neutral_cast_changes_nothing() {
        let frames = CapsuleGlass::new(24, 12, true).render_frames();
        assert_eq!(tint_frames(&frames, Tint::NEUTRAL), frames);
    }
    #[test]
    fn a_blue_cast_moves_the_glass_towards_blue() {
        let frames = CapsuleGlass::new(24, 12, false).render_frames();
        let tinted = tint_frames(&frames, Tint::from_background(20, 40, 220));
        let sum = |fs: &[Vec<u8>], channel: usize| -> u64 {
            fs.iter().flat_map(|f| f.chunks_exact(4)).map(|p| u64::from(p[channel])).sum()
        };
        assert!(sum(&tinted, 0) > sum(&frames, 0));
        assert!(sum(&tinted, 2) < sum(&frames, 2));
    }
    #[test]
    fn blending_clamps_a_mix_outside_the_range() {
        let a = CapsuleGlass::new(16, 8, false).render_frames();
        let b = CapsuleGlass::new(16, 8, true).render_frames();
        assert_eq!(blend_frames(&a, &b, -1.0), a);
        assert_eq!(blend_frames(&a, &b, 2.0), b);
    }
}
