//! Material selection and content protection, independent of GPUI and Win32.
//! Native blur is the first deliverable. Transparent is an explicit A/B diagnostic,
//! never the automatic fallback. A solid capsule remains available without blur.

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Backend { Native, Solid, Transparent }

impl Backend {
    pub fn parse(value: &str) -> Result<Self, String> {
        match value.trim().to_ascii_lowercase().as_str() {
            "native" => Ok(Self::Native),
            "solid" => Ok(Self::Solid),
            "transparent" => Ok(Self::Transparent),
            _ => Err(format!("unknown glass backend {value:?}; expected native, solid or transparent")),
        }
    }

    pub fn resolve(self, native_allowed: bool) -> Self {
        if !native_allowed && self != Self::Transparent { Self::Solid } else { self }
    }
}

pub fn coverage(width: u32, height: u32, x: u32, y: u32) -> f32 {
    let w = width as f32;
    let h = height as f32;
    let radius = w.min(h) * 0.5;
    let qx = (x as f32 + 0.5 - w * 0.5).abs() - (w * 0.5 - radius);
    let qy = (y as f32 + 0.5 - h * 0.5).abs() - (h * 0.5 - radius);
    let distance = qx.max(0.0).hypot(qy.max(0.0)) + qx.max(qy).min(0.0) - radius;
    (0.5 - distance).clamp(0.0, 1.0)
}

/// Compose a neutral content-protection layer UNDER the existing surface shading.
/// Work in the unmasked material, then apply shape coverage exactly once; otherwise
/// two translucent antialiased edges accumulate into a thick rim. Input is BGRA.
pub fn protect_content(
    frames: &mut [Vec<u8>], width: u32, height: u32, dark: bool, backend: Backend,
) -> Result<(), &'static str> {
    let len = (width as usize).checked_mul(height as usize)
        .and_then(|n| n.checked_mul(4)).ok_or("glass dimensions overflow")?;
    if width == 0 || height == 0 || frames.iter().any(|f| f.len() != len) {
        return Err("glass frame dimensions do not match its pixels");
    }
    if backend == Backend::Transparent { return Ok(()); }
    let support_alpha = if backend == Backend::Solid { 1.0 } else { 0.38 };
    let support_bgr = if dark { [36.0, 28.0, 22.0] } else { [252.0, 249.0, 247.0] };
    let mask: Vec<f32> = (0..height).flat_map(|y| (0..width).map(move |x| coverage(width, height, x, y))).collect();
    for frame in frames {
        for (pixel, &mask) in frame.chunks_exact_mut(4).zip(&mask) {
            if mask <= 0.0 { pixel.fill(0); continue; }
            let surface_alpha = (pixel[3] as f32 / 255.0 / mask).clamp(0.0, 1.0);
            let under = support_alpha * (1.0 - surface_alpha);
            let alpha = surface_alpha + under;
            for channel in 0..3 {
                pixel[channel] = ((pixel[channel] as f32 * surface_alpha + support_bgr[channel] * under) / alpha).round().clamp(0.0, 255.0) as u8;
            }
            pixel[3] = (alpha * mask * 255.0).round() as u8;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn glass_backend_parsing_is_explicit() {
        assert_eq!(Backend::parse("native").unwrap(), Backend::Native);
        assert_eq!(Backend::parse(" SOLID ").unwrap(), Backend::Solid);
        assert!(Backend::parse("custom").is_err());
    }
    #[test]
    fn glass_fallback_never_selects_clear_transparency() {
        assert_eq!(Backend::Native.resolve(false), Backend::Solid);
        assert_eq!(Backend::Native.resolve(true), Backend::Native);
        assert_eq!(Backend::Solid.resolve(true), Backend::Solid);
    }
    #[test]
    fn glass_diagnostic_preserves_the_baseline() {
        let original = vec![vec![10, 20, 30, 70].repeat(108 * 26)];
        let mut actual = original.clone();
        protect_content(&mut actual, 108, 26, false, Backend::Transparent).unwrap();
        assert_eq!(actual, original);
    }
    #[test]
    fn glass_solid_center_cannot_leak_original_desktop_text() {
        let mut frames = vec![vec![10, 20, 30, 70].repeat(108 * 26)];
        protect_content(&mut frames, 108, 26, false, Backend::Solid).unwrap();
        assert_eq!(frames[0][(13 * 108 + 54) * 4 + 3], 255);
        assert_eq!(&frames[0][..4], &[0, 0, 0, 0]);
    }
    #[test]
    fn glass_native_protects_content_without_becoming_solid() {
        let mut frames = vec![vec![10, 20, 30, 70].repeat(108 * 26)];
        protect_content(&mut frames, 108, 26, false, Backend::Native).unwrap();
        let alpha = frames[0][(13 * 108 + 54) * 4 + 3];
        assert!(alpha > 130 && alpha < 160);
    }
    #[test]
    fn glass_solid_applies_edge_coverage_once() {
        let mut frames = vec![vec![0; 108 * 26 * 4]];
        protect_content(&mut frames, 108, 26, false, Backend::Solid).unwrap();
        for y in 0..26 { for x in 0..108 {
            assert_eq!(frames[0][((y * 108 + x) * 4 + 3) as usize], (coverage(108, 26, x, y) * 255.0).round() as u8);
        } }
    }
    #[test]
    fn glass_rejects_invalid_pixel_buffers() {
        assert!(protect_content(&mut [vec![0; 3]], 1, 1, false, Backend::Solid).is_err());
        assert!(protect_content(&mut [], 0, 0, false, Backend::Solid).is_err());
    }
}
