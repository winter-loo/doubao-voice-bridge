//! Deterministic CPU reference for a future GPU backdrop material.
//! This is NOT a live desktop capturer and is NOT the production render path.
//! All processing is on supplied, linear-light RGB pixels. Foreground content is
//! composed later. The lens interior has alpha 1: never leak the sharp background
//! back through a second time. Parameters are an artistic model, not Apple's code.

#[derive(Clone, Debug)]
pub struct Image { pub width: u32, pub height: u32, pub pixels: Vec<[f32; 3]> }
const MAX_PIXELS: usize = 4_194_304;

pub fn to_linear(value: u8) -> f32 {
    let v = value as f32 / 255.0;
    if v <= 0.04045 { v / 12.92 } else { ((v + 0.055) / 1.055).powf(2.4) }
}
pub fn to_srgb(value: f32) -> u8 {
    let v = value.clamp(0.0, 1.0);
    let v = if v <= 0.0031308 { 12.92 * v } else { 1.055 * v.powf(1.0 / 2.4) - 0.055 };
    (v * 255.0).round().clamp(0.0, 255.0) as u8
}
impl Image {
    pub fn from_rgb8(width: u32, height: u32, pixels: &[u8]) -> Result<Self, String> {
        let len = (width as usize).checked_mul(height as usize).ok_or("image size overflow")?;
        if len == 0 || len > MAX_PIXELS || pixels.len() != len * 3 { return Err("invalid or excessive image dimensions".into()); }
        Ok(Self { width, height, pixels: pixels.chunks_exact(3).map(|p| [to_linear(p[0]), to_linear(p[1]), to_linear(p[2])]).collect() })
    }
    pub fn rgb8(&self) -> Vec<u8> { self.pixels.iter().flat_map(|p| p.map(to_srgb)).collect() }
    fn valid(&self) -> bool {
        self.width != 0 && self.height != 0 && self.pixels.len() <= MAX_PIXELS
            && self.width as u64 * self.height as u64 == self.pixels.len() as u64
            && self.pixels.iter().flatten().all(|v| v.is_finite() && (0.0..=1.0).contains(v))
    }
    fn at(&self, x: i32, y: i32) -> [f32; 3] {
        self.pixels[y.clamp(0, self.height as i32 - 1) as usize * self.width as usize + x.clamp(0, self.width as i32 - 1) as usize]
    }
    fn sample(&self, x: f32, y: f32) -> [f32; 3] {
        let x = x.clamp(0.0, self.width as f32 - 1.0);
        let y = y.clamp(0.0, self.height as f32 - 1.0);
        let ix = x.floor() as i32; let iy = y.floor() as i32;
        let a = mix(self.at(ix, iy), self.at(ix + 1, iy), x - ix as f32);
        let b = mix(self.at(ix, iy + 1), self.at(ix + 1, iy + 1), x - ix as f32);
        mix(a, b, y - iy as f32)
    }
}
fn mix(a: [f32; 3], b: [f32; 3], t: f32) -> [f32; 3] { std::array::from_fn(|i| a[i] + (b[i] - a[i]) * t) }
fn smooth(t: f32) -> f32 { let t = t.clamp(0.0, 1.0); t * t * (3.0 - 2.0 * t) }

/// Separable Gaussian, normalized and clamped at image edges. Both passes use
/// linear-light colors. Max sigma matches the shader's bounded 64-tap radius.
pub fn blur(source: &Image, sigma: f32) -> Result<Image, String> {
    if !source.valid() || !sigma.is_finite() || !(0.0..=20.0).contains(&sigma) { return Err("invalid image or blur sigma (0..20)".into()); }
    if sigma < 0.01 { return Ok(source.clone()); }
    let radius = (sigma * 3.0).ceil() as i32;
    let weights: Vec<f32> = (-radius..=radius).map(|i| (-(i * i) as f32 / (2.0 * sigma * sigma)).exp()).collect();
    let sum: f32 = weights.iter().sum();
    let weights: Vec<f32> = weights.iter().map(|w| w / sum).collect();
    let mut intermediate = source.clone();
    let mut output = source.clone();
    for (input, target, horizontal) in [(source, &mut intermediate, true)] {
        for y in 0..source.height as i32 { for x in 0..source.width as i32 {
            let mut color = [0.0; 3];
            for (index, weight) in weights.iter().enumerate() {
                let offset = index as i32 - radius;
                let pixel = input.at(x + if horizontal { offset } else { 0 }, y);
                for c in 0..3 { color[c] += pixel[c] * weight; }
            }
            target.pixels[y as usize * source.width as usize + x as usize] = color.map(|v| v.clamp(0.0, 1.0));
        } }
    }
    for y in 0..source.height as i32 { for x in 0..source.width as i32 {
        let mut color = [0.0; 3];
        for (index, weight) in weights.iter().enumerate() {
            let pixel = intermediate.at(x, y + index as i32 - radius);
            for c in 0..3 { color[c] += pixel[c] * weight; }
        }
        output.pixels[y as usize * source.width as usize + x as usize] = color.map(|v| v.clamp(0.0, 1.0));
    } }
    Ok(output)
}

#[derive(Clone, Copy, Debug)]
pub enum Theme { Light, Dark }
#[derive(Clone, Copy, Debug)]
pub struct Capsule { pub left: u32, pub top: u32, pub width: u32, pub height: u32 }
#[derive(Clone, Copy, Debug)]
pub struct Parameters {
    pub bevel_ratio: f32,
    pub refraction_ratio: f32,
    pub wide_blur_ratio: f32,
    pub narrow_blur_ratio: f32,
    pub saturation: f32,
}
impl Default for Parameters {
    fn default() -> Self { Self { bevel_ratio: 0.30, refraction_ratio: 0.10, wide_blur_ratio: 0.12, narrow_blur_ratio: 0.035, saturation: 0.80 } }
}
impl Parameters {
    fn valid(&self) -> bool {
        self.bevel_ratio.is_finite() && (0.05..=0.90).contains(&self.bevel_ratio)
            && self.refraction_ratio.is_finite() && (0.0..=0.30).contains(&self.refraction_ratio)
            && self.wide_blur_ratio.is_finite() && (0.01..=0.30).contains(&self.wide_blur_ratio)
            && self.narrow_blur_ratio.is_finite() && (0.0..=self.wide_blur_ratio).contains(&self.narrow_blur_ratio)
            && self.saturation.is_finite() && (0.0..=1.0).contains(&self.saturation)
    }
}

/// Signed distance and 2D normal for a horizontal capsule. Coordinates are local
/// device-pixel centers. This intentionally shares the GPU shader's geometry.
pub fn field(capsule: Capsule, x: f32, y: f32) -> (f32, f32, f32) {
    let radius = capsule.height as f32 * 0.5;
    let half_segment = (capsule.width - capsule.height) as f32 * 0.5;
    let qx = x - capsule.width as f32 * 0.5;
    let qy = y - radius;
    let dx = qx - qx.clamp(-half_segment, half_segment);
    let length = dx.hypot(qy);
    if length < 1e-6 { return (-radius, 0.0, 0.0); }
    (length - radius, dx / length, qy / length)
}
fn displacement(capsule: Capsule, params: Parameters, x: f32, y: f32) -> (f32, f32, f32, f32) {
    let (distance, nx, ny) = field(capsule, x, y);
    let bevel = (capsule.height as f32 * 0.5 * params.bevel_ratio).max(1.0);
    let edge = smooth(1.0 + distance / bevel);
    let amount = capsule.height as f32 * params.refraction_ratio * edge;
    (nx * amount, ny * amount, edge, distance)
}

#[derive(Clone, Debug)]
pub struct Layer { pub width: u32, pub height: u32, pub pixels: Vec<[f32; 4]> }
impl Layer {
    pub fn rgba8(&self) -> Vec<u8> {
        self.pixels.iter().flat_map(|p| [to_srgb(p[0]), to_srgb(p[1]), to_srgb(p[2]), (p[3].clamp(0.0, 1.0) * 255.0).round() as u8]).collect()
    }
}
fn validate_capsule(source: &Image, capsule: Capsule) -> Result<(), String> {
    if !source.valid() || capsule.height < 4 || capsule.width < capsule.height
        || capsule.width > 4096 || capsule.height > 512
        || capsule.left as u64 + capsule.width as u64 > source.width as u64
        || capsule.top as u64 + capsule.height as u64 > source.height as u64 {
        return Err("capsule must fit inside a valid input image (horizontal, height 4..512)".into());
    }
    Ok(())
}

pub fn render(source: &Image, capsule: Capsule, theme: Theme, params: Parameters) -> Result<Layer, String> {
    validate_capsule(source, capsule)?;
    if !params.valid() { return Err("invalid material parameters".into()); }
    let wide = blur(source, capsule.height as f32 * params.wide_blur_ratio)?;
    let narrow = blur(source, capsule.height as f32 * params.narrow_blur_ratio)?;
    let (tint, opacity) = match theme {
        Theme::Light => ([0.93, 0.95, 0.98], 0.50),
        Theme::Dark => ([0.015, 0.020, 0.030], 0.60),
    };
    let mut pixels = Vec::with_capacity(capsule.width as usize * capsule.height as usize);
    for y in 0..capsule.height { for x in 0..capsule.width {
        let px = x as f32 + 0.5; let py = y as f32 + 0.5;
        let (dx, dy, edge, distance) = displacement(capsule, params, px, py);
        let coverage = (0.5 - distance).clamp(0.0, 1.0);
        if coverage == 0.0 { pixels.push([0.0; 4]); continue; }
        let sx = capsule.left as f32 + px - dx - 0.5;
        let sy = capsule.top as f32 + py - dy - 0.5;
        let mut color = mix(wide.sample(sx, sy), narrow.sample(sx, sy), edge * 0.60);
        let luminance = 0.2126 * color[0] + 0.7152 * color[1] + 0.0722 * color[2];
        color = mix([luminance; 3], color, params.saturation);
        color = mix(color, tint, opacity);
        let (_, nx, ny) = field(capsule, px, py);
        let facing = (-0.3047757 * nx - 0.9524242 * ny).max(0.0);
        let scale = capsule.height as f32 / 26.0;
        let highlight = (-((distance + 1.55 * scale) / (0.60 * scale)).powi(2)).exp();
        let key = 0.28 * facing.powf(2.2) * highlight;
        let shadow = 0.08 * edge * (1.0 - facing);
        color = color.map(|v| (v * (1.0 - shadow) + key).clamp(0.0, 1.0));
        pixels.push([color[0], color[1], color[2], coverage]);
    } }
    Ok(Layer { width: capsule.width, height: capsule.height, pixels })
}

/// Alpha coverage is used ONLY for the silhouette. Central pixels replace the
/// unprocessed scene; source alpha must not be applied to transmission twice.
pub fn composite(source: &Image, capsule: Capsule, layer: &Layer) -> Result<Image, String> {
    validate_capsule(source, capsule)?;
    if layer.width != capsule.width || layer.height != capsule.height
        || layer.pixels.len() != layer.width as usize * layer.height as usize
        || !layer.pixels.iter().flatten().all(|v| v.is_finite() && (0.0..=1.0).contains(v)) {
        return Err("invalid material layer".into());
    }
    let mut output = source.clone();
    for y in 0..capsule.height { for x in 0..capsule.width {
        let pixel = layer.pixels[(y * capsule.width + x) as usize];
        let index = ((capsule.top + y) * source.width + capsule.left + x) as usize;
        output.pixels[index] = mix(source.pixels[index], [pixel[0], pixel[1], pixel[2]], pixel[3]);
    } }
    Ok(output)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn flat(value: u8) -> Image { Image::from_rgb8(160, 64, &vec![value; 160 * 64 * 3]).unwrap() }
    fn cap() -> Capsule { Capsule { left: 20, top: 16, width: 120, height: 32 } }
    #[test]
    fn srgb_round_trip_preserves_all_8_bit_values() { for v in 0..=255 { assert_eq!(to_srgb(to_linear(v)), v); } }
    #[test]
    fn blur_preserves_a_constant_field() {
        let image = flat(128); let blurred = blur(&image, 3.0).unwrap();
        assert!(blurred.pixels.iter().all(|p| (p[0] - image.pixels[0][0]).abs() < 1e-5));
    }
    #[test]
    fn zero_sigma_is_an_identity() { let image = flat(83); assert_eq!(blur(&image, 0.0).unwrap().pixels, image.pixels); }
    #[test]
    fn actual_input_pixels_change_the_material() {
        let a = render(&flat(0), cap(), Theme::Light, Parameters::default()).unwrap();
        let b = render(&flat(255), cap(), Theme::Light, Parameters::default()).unwrap();
        assert!(b.pixels[16 * 120 + 60][0] - a.pixels[16 * 120 + 60][0] > 0.4);
    }
    #[test]
    fn lens_center_is_opaque_and_corners_are_transparent() {
        let layer = render(&flat(128), cap(), Theme::Light, Parameters::default()).unwrap();
        assert_eq!(layer.pixels[16 * 120 + 60][3], 1.0);
        assert_eq!(layer.pixels[0][3], 0.0);
    }
    #[test]
    fn sharp_text_cannot_leak_back_through_opaque_interior() {
        let scene = flat(0); let layer = render(&scene, cap(), Theme::Light, Parameters::default()).unwrap();
        let result = composite(&scene, cap(), &layer).unwrap();
        assert_eq!(result.pixels[32 * 160 + 80], layer.pixels[16 * 120 + 60][..3]);
    }
    #[test]
    fn the_flat_center_has_no_refraction() {
        let (x,y,_,_) = displacement(cap(), Parameters::default(), 60.0, 16.0);
        assert_eq!((x,y), (0.0,0.0));
    }
    #[test]
    fn top_and_flank_refraction_follow_the_shape_normal() {
        let (x,y,_,_) = displacement(cap(), Parameters::default(), 60.0, 0.5);
        assert_eq!(x, 0.0); assert!(y < -1.0);
        let (x,y,_,_) = displacement(cap(), Parameters::default(), 0.5, 16.0);
        assert!(x < -1.0); assert_eq!(y, 0.0);
    }
    #[test]
    fn fine_stripes_lose_high_frequency_contrast() {
        let mut source = flat(0);
        for y in 0..64 { for x in 0..160 { source.pixels[y*160+x] = [(x%2) as f32;3]; } }
        let layer = render(&source, cap(), Theme::Light, Parameters::default()).unwrap();
        let mut largest: f32 = 0.0;
        for x in 30..90 { largest = largest.max((layer.pixels[16*120+x][0]-layer.pixels[16*120+x+1][0]).abs()); }
        assert!(largest < 0.01, "fine stripe contrast survived: {largest}");
    }
    #[test]
    fn outside_the_capsule_is_unchanged() {
        let source = flat(128); let layer = render(&source, cap(), Theme::Dark, Parameters::default()).unwrap();
        let result = composite(&source, cap(), &layer).unwrap();
        assert_eq!(source.pixels[0], result.pixels[0]);
        assert_eq!(source.pixels[16*160+20], result.pixels[16*160+20]);
    }
    #[test]
    fn invalid_dimensions_and_nan_are_rejected() {
        assert!(Image::from_rgb8(2,2,&[0;3]).is_err());
        assert!(render(&flat(128), Capsule { width: 2, ..cap() }, Theme::Light, Parameters::default()).is_err());
        assert!(render(&flat(128), cap(), Theme::Light, Parameters { refraction_ratio: f32::NAN, ..Parameters::default() }).is_err());
        assert!(blur(&flat(128), 21.0).is_err());
    }
    #[test]
    fn light_and_dark_are_distinct_materials() {
        let a = render(&flat(128), cap(), Theme::Light, Parameters::default()).unwrap();
        let b = render(&flat(128), cap(), Theme::Dark, Parameters::default()).unwrap();
        assert!(a.pixels[16*120+60][0] > b.pixels[16*120+60][0] + 0.3);
    }
}
