//! Procedural liquid-glass shading for the voice overlay capsule.
//!
//! This is a port of the pure CSS + SVG technique. There, a `<canvas>` computes a
//! displacement map from a rounded-rect signed distance field at runtime, and
//! `backdrop-filter` feeds the blurred backdrop through an inline `feDisplacementMap`
//! that reads it. GPUI exposes no backdrop texture, so the second half of that
//! pipeline is unavailable — but the first half, the part that actually produces the
//! optics, is just math over the same distance field.
//!
//! So we compute the identical field here (SDF, surface normal, bevel profile,
//! displacement vector) and shade the lens directly instead of handing the vector to
//! a filter primitive. Every visual layer the CSS version stacks as a pseudo-element
//! becomes one `over()` composite below, driven by the same `rim` term.
//!
//! The result is one straight-alpha BGRA bitmap per animation frame, uploaded once and
//! cached, so per-frame cost stays at "draw a sprite".
//!
//! This module deliberately depends on nothing but `core`, which keeps the optics
//! unit-testable and lets it be compiled standalone with `rustc --test`.

/// Number of sheen phases baked into the animated texture. One full traversal of the
/// capsule per overlay animation loop.
pub const SHEEN_FRAMES: usize = 24;

/// Rim thickness as a fraction of the capsule's corner radius. The bevel is the band
/// where the glass surface curves away and bends light; inside it the slab is flat.
const BEVEL_RATIO: f32 = 0.46;

/// Peak displacement as a fraction of the bevel width. This is the `scale` attribute
/// of `feDisplacementMap` in the CSS version.
const DISPLACEMENT_RATIO: f32 = 0.55;

/// Sharpness of the bevel cross-section. 2.0 is a circular dome; higher values flatten
/// the top and concentrate the bend into the outermost sliver, which is what reads as
/// "thin hard glass" rather than "plastic bubble".
const BEVEL_SHARPNESS: f32 = 3.9;

/// Direction the key light comes from, in texture space (y grows downward). Tilted off
/// vertical so the highlight peaks left of top and the capsule does not look symmetric
/// and flat.
const LIGHT: (f32, f32) = (-0.32, -1.0);

/// Extra optical path near the rim: light crossing the bevel travels through more
/// glass, so the tint deepens there.
const THICKNESS_GAIN: f32 = 0.52;

/// Half-width, in device pixels, of the bright line riding the outer boundary.
const EDGE_WIDTH: f32 = 1.15;

/// How far inside the boundary that line sits, in device pixels.
const EDGE_INSET: f32 = 0.85;

/// Half-width of the travelling sheen, as a fraction of the capsule width.
const SHEEN_WIDTH: f32 = 0.17;

/// Straight-alpha color. Channels are in GPUI's convention, where an `rgba()` literal's
/// bytes are fed to the GPU unconverted, so these match the palette used by the quads
/// this module replaces.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Rgba {
    pub r: f32,
    pub g: f32,
    pub b: f32,
    pub a: f32,
}

impl Rgba {
    const TRANSPARENT: Self = Self {
        r: 0.0,
        g: 0.0,
        b: 0.0,
        a: 0.0,
    };

    /// Build from a `0xRRGGBBAA` literal, the same spelling GPUI's `rgba()` takes.
    const fn hex(value: u32) -> Self {
        Self {
            r: ((value >> 24) & 0xff) as f32 / 255.0,
            g: ((value >> 16) & 0xff) as f32 / 255.0,
            b: ((value >> 8) & 0xff) as f32 / 255.0,
            a: (value & 0xff) as f32 / 255.0,
        }
    }

    fn with_alpha(self, alpha: f32) -> Self {
        Self {
            a: alpha.clamp(0.0, 1.0),
            ..self
        }
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

    /// Source-over composite in straight alpha. Stacking these reproduces what the CSS
    /// version gets from stacking translucent pseudo-elements.
    fn over(self, under: Self) -> Self {
        let out_a = self.a + under.a * (1.0 - self.a);
        if out_a <= f32::EPSILON {
            return Self::TRANSPARENT;
        }
        let blend =
            |top: f32, bottom: f32| (top * self.a + bottom * under.a * (1.0 - self.a)) / out_a;
        Self {
            r: blend(self.r, under.r),
            g: blend(self.g, under.g),
            b: blend(self.b, under.b),
            a: out_a,
        }
    }
}

/// The palette for one lighting environment. Two instances exist, picked by whatever
/// the overlay sampled from the desktop behind it.
struct Palette {
    tint_top: Rgba,
    tint_bottom: Rgba,
    /// Darkening of the inner wall, just inside the bevel, that gives the slab depth.
    inner_wall: Rgba,
    /// The two dispersion tints. Light leaving the rim splits, cool on one flank and
    /// warm on the other.
    dispersion_cool: Rgba,
    dispersion_warm: Rgba,
    /// Specular response of the rim to the key light and to bounce from below.
    key_light: Rgba,
    bounce_light: Rgba,
    edge_line: Rgba,
    sheen: Rgba,
}

const DARK_PALETTE: Palette = Palette {
    tint_top: Rgba::hex(0x5f7f9660),
    tint_bottom: Rgba::hex(0x07121f8a),
    inner_wall: Rgba::hex(0x040c1626),
    dispersion_cool: Rgba::hex(0x8ee9ff8c),
    dispersion_warm: Rgba::hex(0xffc8f166),
    key_light: Rgba::hex(0xffffffec),
    bounce_light: Rgba::hex(0xa7dcff68),
    edge_line: Rgba::hex(0xffffffff),
    sheen: Rgba::hex(0xdff2ff1c),
};

/// The light palette cannot mirror the dark one.
///
/// The dark palette separates from its background in both directions at once: a body
/// darker than the desktop and a rim brighter than it. Over a white document there is no
/// "brighter" left to use, so a palette built the same way -- white tint, white rim,
/// white bounce -- disappears into the page. Glass on paper reads the other way round:
/// what you see is the edge bending light *away*, so the rim is darker than the surface,
/// not lighter, and the body carries a faint cool cast rather than a white wash.
const LIGHT_PALETTE: Palette = Palette {
    tint_top: Rgba::hex(0xdae6f27e),
    tint_bottom: Rgba::hex(0x9db4cb96),
    inner_wall: Rgba::hex(0x2c455c5e),
    dispersion_cool: Rgba::hex(0x3dbcff70),
    dispersion_warm: Rgba::hex(0xff86cf58),
    key_light: Rgba::hex(0xffffffe6),
    bounce_light: Rgba::hex(0xeaf4ff8c),
    edge_line: Rgba::hex(0x51698adc),
    sheen: Rgba::hex(0xffffff40),
};

/// A capsule-shaped glass lens, measured in device pixels.
pub struct CapsuleGlass {
    width: f32,
    height: f32,
    radius: f32,
    bevel: f32,
    displacement: f32,
    palette: &'static Palette,
}

impl CapsuleGlass {
    /// `width` and `height` are device pixels, so the caller multiplies logical size by
    /// the window scale factor before calling. The lens is a full capsule: the corner
    /// radius is half the height.
    pub fn new(width: u32, height: u32, dark: bool) -> Self {
        let width = width.max(1) as f32;
        let height = height.max(1) as f32;
        let radius = (width.min(height)) / 2.0;
        let bevel = (radius * BEVEL_RATIO).max(1.0);
        Self {
            width,
            height,
            radius,
            bevel,
            displacement: bevel * DISPLACEMENT_RATIO,
            palette: if dark { &DARK_PALETTE } else { &LIGHT_PALETTE },
        }
    }

    /// Signed distance to the capsule boundary plus the outward unit normal there.
    /// Negative distance is inside. This is the field the CSS version's canvas walks to
    /// build its displacement map.
    fn field(&self, x: f32, y: f32) -> (f32, f32, f32) {
        let qx = x - self.width / 2.0;
        let qy = y - self.height / 2.0;
        let ax = qx.abs() - (self.width / 2.0 - self.radius);
        let ay = qy.abs() - (self.height / 2.0 - self.radius);
        let sx = if qx < 0.0 { -1.0 } else { 1.0 };
        let sy = if qy < 0.0 { -1.0 } else { 1.0 };

        if ax > 0.0 && ay > 0.0 {
            let len = (ax * ax + ay * ay).sqrt();
            if len <= f32::EPSILON {
                return (-self.radius, sx, 0.0);
            }
            (len - self.radius, sx * ax / len, sy * ay / len)
        } else if ax > ay {
            (ax - self.radius, sx, 0.0)
        } else {
            (ay - self.radius, 0.0, sy)
        }
    }

    /// What the slab looks like before the rim bends anything: a vertical tint gradient.
    ///
    /// This is the single point where the lens reads its content. A backdrop-sampling
    /// build — Windows acrylic, or a KWin blur surface — would replace this body and
    /// leave every other layer untouched.
    fn sample_interior(&self, _x: f32, y: f32) -> Rgba {
        let v = (y / self.height).clamp(0.0, 1.0);
        self.palette.tint_top.lerp(self.palette.tint_bottom, v)
    }

    /// Shade one pixel of one sheen phase, returning straight-alpha color.
    pub fn shade(&self, x: f32, y: f32, phase: f32) -> Rgba {
        let (distance, nx, ny) = self.field(x, y);
        let coverage = (0.5 - distance).clamp(0.0, 1.0);
        if coverage <= 0.0 {
            return Rgba::TRANSPARENT;
        }

        // Position across the bevel: 0 where the slab is still flat, 1 at the boundary.
        let t = (1.0 + distance / self.bevel).clamp(0.0, 1.0);
        let rim = bevel_profile(t);

        // The displacement vector. In the CSS version this is what gets packed into the
        // red and green channels of the map; here we apply it ourselves.
        let offset = rim * self.displacement;
        let sample_x = x - nx * offset;
        let sample_y = y - ny * offset;

        // How square-on the rim faces the key light. Drives every specular term, so the
        // whole highlight set stays consistent with one light position.
        let light_len = (LIGHT.0 * LIGHT.0 + LIGHT.1 * LIGHT.1).sqrt();
        let facing = ((nx * LIGHT.0 + ny * LIGHT.1) / light_len).max(0.0);

        // 1. Refracted body. Sampling at the displaced point compresses the gradient
        //    into the rim, and the extra optical path there deepens the tint.
        let mut color = self.sample_interior(sample_x, sample_y);
        color = color.with_alpha(color.a * (1.0 + THICKNESS_GAIN * rim));

        // 2. Inner wall: a soft dark ring just inside the bevel, the thickness cue.
        let wall = gaussian(t - 0.34, 0.26) * (1.0 - 0.6 * facing);
        color = self
            .palette
            .inner_wall
            .with_alpha(self.palette.inner_wall.a * wall)
            .over(color);

        // 3. Chromatic dispersion across the bevel, cool on the left flank and warm on
        //    the right. Because it is driven by the normal it wraps the round ends
        //    instead of stopping at a rectangle's edge.
        let cool = (-nx).max(0.0);
        let warm = nx.max(0.0);
        let dispersion = self
            .palette
            .dispersion_cool
            .with_alpha(self.palette.dispersion_cool.a * rim * cool)
            .over(
                self.palette
                    .dispersion_warm
                    .with_alpha(self.palette.dispersion_warm.a * rim * warm),
            );
        color = dispersion.over(color);

        // 4. Specular: the key light on the upper rim, plus bounce from the surface the
        //    overlay floats above.
        let bounce = ny.max(0.0).powf(3.0) * rim;
        color = self
            .palette
            .bounce_light
            .with_alpha(self.palette.bounce_light.a * bounce)
            .over(color);
        let key = facing.powf(2.2) * rim;
        color = self
            .palette
            .key_light
            .with_alpha(self.palette.key_light.a * key)
            .over(color);

        // 5. The bright line riding the boundary, dimmed where it turns away from the
        //    light so it reads as a lit rim rather than a drawn stroke.
        let line = gaussian(distance + EDGE_INSET, EDGE_WIDTH) * (0.22 + 0.78 * facing);
        color = self
            .palette
            .edge_line
            .with_alpha(self.palette.edge_line.a * line)
            .over(color);

        // 6. Sheen sweeping along the capsule, slanted so it crosses the rim.
        let travel = -0.3 + 1.6 * phase;
        let along = (x + y * 0.45) / self.width;
        let sheen = gaussian(along - travel, SHEEN_WIDTH) * (0.3 + 0.7 * rim);
        color = self
            .palette
            .sheen
            .with_alpha(self.palette.sheen.a * sheen)
            .over(color);

        color.with_alpha(color.a * coverage)
    }

    /// Render one sheen phase as straight-alpha BGRA, the layout GPUI's `RenderImage`
    /// expects.
    pub fn render_bgra(&self, frame: usize) -> Vec<u8> {
        let width = self.width as usize;
        let height = self.height as usize;
        let phase = (frame % SHEEN_FRAMES) as f32 / SHEEN_FRAMES as f32;
        let mut pixels = Vec::with_capacity(width * height * 4);

        for y in 0..height {
            for x in 0..width {
                let color = self.shade(x as f32 + 0.5, y as f32 + 0.5, phase);
                pixels.push(to_byte(color.b));
                pixels.push(to_byte(color.g));
                pixels.push(to_byte(color.r));
                pixels.push(to_byte(color.a));
            }
        }

        pixels
    }

    /// Every sheen phase, in order.
    pub fn render_frames(&self) -> Vec<Vec<u8>> {
        (0..SHEEN_FRAMES)
            .map(|frame| self.render_bgra(frame))
            .collect()
    }
}

/// Mixes two rendered frame sets into a third, `mix` running from all `start` to all
/// `end`.
///
/// This interpolates the finished pixels rather than the palettes behind them, which is
/// what keeps alpha linear across the range: a capsule halfway between the two is
/// halfway as opaque. Painting one glass on top of the other instead would compound
/// their alpha and make the capsule visibly thicker in the middle of a change -- the
/// thing that separates a transition from a dissolve.
///
/// Both sides must share a frame count and frame length; they come from the same
/// `render_frames` shape, so a mismatch is a programming error rather than input.
/// The mix is carried as a 0..=256 fixed-point weight so each byte costs one multiply,
/// one add and a shift. The float form needed two conversions and a `round` per byte,
/// which is 16ms for a single capsule in an unoptimised build -- a whole frame's budget
/// spent on one step of a transition that has twelve of them.
const BLEND_ONE: u32 = 256;

pub fn blend_frames(start: &[Vec<u8>], end: &[Vec<u8>], mix: f32) -> Vec<Vec<u8>> {
    debug_assert_eq!(start.len(), end.len(), "frame sets differ in length");
    let mix = (mix.clamp(0.0, 1.0) * BLEND_ONE as f32).round() as u32;

    start
        .iter()
        .zip(end)
        .map(|(from, to)| {
            debug_assert_eq!(from.len(), to.len(), "frames differ in size");
            from.iter()
                .zip(to)
                .map(|(&from, &to)| {
                    // The half added before the shift rounds to nearest rather than
                    // always towards zero, which would otherwise drag every mixed
                    // capsule slightly darker than the two ends it sits between.
                    let blended = (u32::from(from) * (BLEND_ONE - mix)
                        + u32::from(to) * mix
                        + BLEND_ONE / 2)
                        >> 8;
                    blended as u8
                })
                .collect()
        })
        .collect()
}

/// Refraction strength across the bevel, from 0 where the slab is flat to 1 at the
/// boundary.
///
/// This is the slope of a squircle cross-section `z = (1 - t^N)^(1/N)`, saturated
/// through `s / (1 + s)` so the mathematically infinite slope at the boundary lands on
/// 1 instead of blowing up. Raising `BEVEL_SHARPNESS` pushes the bend further out and
/// makes the glass read as thinner and harder.
fn bevel_profile(t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    let dome = (1.0 - t.powf(BEVEL_SHARPNESS)).max(1.0e-4);
    let slope = t.powf(BEVEL_SHARPNESS - 1.0) * dome.powf(1.0 / BEVEL_SHARPNESS - 1.0);
    slope / (1.0 + slope)
}

/// Unnormalized gaussian falloff, used wherever a layer needs a soft band.
fn gaussian(offset: f32, width: f32) -> f32 {
    let n = offset / width;
    (-n * n).exp()
}

fn to_byte(value: f32) -> u8 {
    (value.clamp(0.0, 1.0) * 255.0).round() as u8
}

#[cfg(test)]
mod tests {
    use super::*;

    const WIDTH: u32 = 216;
    const HEIGHT: u32 = 52;

    fn glass() -> CapsuleGlass {
        CapsuleGlass::new(WIDTH, HEIGHT, true)
    }

    #[test]
    fn distance_field_matches_capsule_geometry() {
        let glass = glass();
        let center = glass.field(WIDTH as f32 / 2.0, HEIGHT as f32 / 2.0);
        assert!((center.0 + glass.radius).abs() < 0.01, "{center:?}");

        // Straight top edge: the normal points up and distance is the gap to it.
        let (distance, nx, ny) = glass.field(WIDTH as f32 / 2.0, 4.0);
        assert!((distance + 4.0).abs() < 0.01);
        assert_eq!(nx, 0.0);
        assert_eq!(ny, -1.0);

        // Leftmost point of the left cap: the normal points left, distance is zero.
        let (distance, nx, ny) = glass.field(0.0, HEIGHT as f32 / 2.0);
        assert!(distance.abs() < 0.01);
        assert_eq!(nx, -1.0);
        assert_eq!(ny, 0.0);
    }

    #[test]
    fn distance_field_is_positive_outside_the_round_cap() {
        let glass = glass();
        // The texture corner sits outside the capsule silhouette.
        let (distance, _, _) = glass.field(0.5, 0.5);
        assert!(distance > 0.0, "corner should be outside: {distance}");
    }

    #[test]
    fn bevel_profile_runs_from_flat_to_full_bend() {
        assert_eq!(bevel_profile(0.0), 0.0);
        assert!(bevel_profile(1.0) > 0.99);
        // Monotonic, and weighted toward the outer half of the bevel.
        let mut previous = 0.0;
        for step in 0..=100 {
            let value = bevel_profile(step as f32 / 100.0);
            assert!(value >= previous, "not monotonic at {step}");
            previous = value;
        }
        assert!(bevel_profile(0.5) < 0.25, "bend should stay near the edge");
    }

    #[test]
    fn corners_are_transparent_and_the_slab_is_not() {
        let glass = glass();
        assert_eq!(glass.shade(0.5, 0.5, 0.0).a, 0.0);
        assert!(glass.shade(WIDTH as f32 / 2.0, HEIGHT as f32 / 2.0, 0.0).a > 0.1);
    }

    #[test]
    fn rim_is_brighter_than_the_slab_interior() {
        let glass = glass();
        let center = glass.shade(WIDTH as f32 / 2.0, HEIGHT as f32 / 2.0, 0.5);
        let top_rim = glass.shade(WIDTH as f32 / 2.0, 1.2, 0.5);
        let luminance = |c: Rgba| c.r * 0.2126 + c.g * 0.7152 + c.b * 0.0722;
        assert!(
            luminance(top_rim) > luminance(center) + 0.2,
            "rim {:?} vs center {:?}",
            top_rim,
            center
        );
        assert!(top_rim.a > center.a);
    }

    #[test]
    fn the_light_capsule_separates_from_a_white_page() {
        // The regression this guards against: a light palette built the way the dark one
        // is -- white tint, white rim, white bounce -- vanishes on a white document,
        // because against white there is no "brighter than the background" left to use.
        // Composited onto the page, the rim has to come out materially *darker* than it.
        let glass = CapsuleGlass::new(WIDTH, HEIGHT, false);
        let luminance = |c: Rgba| c.r * 0.2126 + c.g * 0.7152 + c.b * 0.0722;
        let over_white = |c: Rgba| luminance(c) * c.a + (1.0 - c.a);

        let rim = over_white(glass.shade(WIDTH as f32 / 2.0, 1.2, 0.5));
        let body = over_white(glass.shade(WIDTH as f32 / 2.0, HEIGHT as f32 / 2.0, 0.5));
        assert!(rim < 0.80, "rim over white is {rim}, too close to the page");
        assert!(body < 0.93, "body over white is {body}, too close to the page");
        assert!(rim < body, "the rim has to read darker than the body it encloses");
    }

    #[test]
    fn each_palette_leans_away_from_its_own_background() {
        // The two palettes are not mirror images: the dark one is legible because its rim
        // is brighter than the desktop behind it, the light one because its rim is
        // darker. Losing either direction is what makes a capsule disappear.
        let luminance = |c: Rgba| c.r * 0.2126 + c.g * 0.7152 + c.b * 0.0722;
        let rim_of = |dark: bool| {
            let glass = CapsuleGlass::new(WIDTH, HEIGHT, dark);
            glass.shade(WIDTH as f32 / 2.0, 1.2, 0.5)
        };

        let on_black = |c: Rgba| luminance(c) * c.a;
        let on_white = |c: Rgba| luminance(c) * c.a + (1.0 - c.a);
        assert!(on_black(rim_of(true)) > 0.5, "the dark rim must light up a dark desktop");
        assert!(on_white(rim_of(false)) < 0.8, "the light rim must darken a light one");
    }

    #[test]
    fn highlight_wraps_the_round_cap() {
        // The rim treatment follows the normal, so a point on the curved cap that faces
        // the light is lit — the axis-aligned gradients this replaces could not do that.
        let glass = glass();
        let radius = glass.radius;
        let angle = 2.4_f32; // up and to the left, on the left cap
        let cap = glass.shade(
            radius + (radius - 1.0) * -angle.sin(),
            radius - (radius - 1.0) * angle.cos(),
            0.5,
        );
        let interior = glass.shade(WIDTH as f32 / 2.0, HEIGHT as f32 / 2.0, 0.5);
        assert!(cap.a > interior.a, "cap {:?} interior {:?}", cap, interior);
    }

    #[test]
    fn dispersion_splits_across_the_two_flanks() {
        let glass = glass();
        let y = HEIGHT as f32 / 2.0;
        let left = glass.shade(1.0, y, 0.5);
        let right = glass.shade(WIDTH as f32 - 1.0, y, 0.5);
        // Cool flank keeps more blue than red; warm flank does the opposite.
        assert!(left.b - left.r > right.b - right.r, "{left:?} {right:?}");
    }

    #[test]
    fn sheen_moves_between_phases() {
        let glass = glass();
        let probe = |phase: f32| glass.shade(WIDTH as f32 * 0.25, HEIGHT as f32 * 0.3, phase);
        assert!((probe(0.0).a - probe(0.5).a).abs() > 1.0e-4);
    }

    #[test]
    fn rendered_frames_have_the_expected_shape() {
        let glass = CapsuleGlass::new(40, 20, false);
        let frames = glass.render_frames();
        assert_eq!(frames.len(), SHEEN_FRAMES);
        for frame in &frames {
            assert_eq!(frame.len(), 40 * 20 * 4);
        }
        // Top-left texture corner is outside the capsule in every frame.
        for frame in &frames {
            assert_eq!(frame[3], 0);
        }
    }

    #[test]
    fn over_composites_like_source_over() {
        let opaque = Rgba::hex(0xff0000ff);
        let clear = Rgba::TRANSPARENT;
        assert_eq!(opaque.over(clear), opaque);
        assert_eq!(clear.over(opaque), opaque);

        let half_white = Rgba::hex(0xffffff80);
        let black = Rgba::hex(0x000000ff);
        let blended = half_white.over(black);
        assert!((blended.a - 1.0).abs() < 1.0e-6);
        assert!((blended.r - 128.0 / 255.0).abs() < 0.01, "{blended:?}");
    }

    #[test]
    fn blending_returns_each_end_untouched() {
        let light = CapsuleGlass::new(24, 12, false).render_frames();
        let dark = CapsuleGlass::new(24, 12, true).render_frames();
        assert_eq!(blend_frames(&light, &dark, 0.0), light);
        assert_eq!(blend_frames(&light, &dark, 1.0), dark);
    }

    #[test]
    fn blending_stays_between_the_two_ends() {
        let light = CapsuleGlass::new(24, 12, false).render_frames();
        let dark = CapsuleGlass::new(24, 12, true).render_frames();
        let middle = blend_frames(&light, &dark, 0.5);

        for (frame, (light, dark)) in middle.iter().zip(light.iter().zip(&dark)) {
            for ((&mixed, &light), &dark) in frame.iter().zip(light).zip(dark) {
                let low = light.min(dark);
                let high = light.max(dark);
                assert!(
                    (low..=high).contains(&mixed),
                    "{mixed} escaped the range {low}..={high}"
                );
            }
        }
    }

    #[test]
    fn alpha_moves_linearly_so_the_capsule_never_thickens() {
        // A capsule painted over itself would compound alpha; mixing must not. Halfway
        // through, every pixel's alpha has to sit on the straight line between the ends.
        let light = CapsuleGlass::new(24, 12, false).render_frames();
        let dark = CapsuleGlass::new(24, 12, true).render_frames();
        let middle = blend_frames(&light, &dark, 0.5);

        for (frame, (light, dark)) in middle.iter().zip(light.iter().zip(&dark)) {
            for offset in (3..frame.len()).step_by(4) {
                let expected = (f32::from(light[offset]) + f32::from(dark[offset])) / 2.0;
                let actual = f32::from(frame[offset]);
                assert!(
                    (actual - expected).abs() <= 0.5,
                    "alpha {actual} is not the midpoint {expected}"
                );
            }
        }
    }

    #[test]
    fn blending_clamps_a_mix_outside_the_range() {
        let light = CapsuleGlass::new(16, 8, false).render_frames();
        let dark = CapsuleGlass::new(16, 8, true).render_frames();
        assert_eq!(blend_frames(&light, &dark, -1.0), light);
        assert_eq!(blend_frames(&light, &dark, 2.0), dark);
    }
}
