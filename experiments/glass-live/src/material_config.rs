//! Issue #11, stage 1: one parameter source for CPU filters and HLSL constants.
//! This is a behavior-preserving refactor. All RGB/tone values are linear-light,
//! not sRGB hex colors. Alpha in the final shader remains silhouette coverage.
use std::fmt::Write;

#[derive(Clone, Copy, Debug)]
pub struct BlurConfig {
    /// Sigma is a fraction of physical capsule height; identical for both themes.
    pub center_fraction: f32,
    pub edge_fraction: f32,
    pub max_sigma_pixels: f32,
    pub padding_fraction: f32,
}
pub const BLUR: BlurConfig = BlurConfig {
    center_fraction: 0.20, edge_fraction: 0.045,
    max_sigma_pixels: 20.0, padding_fraction: 0.65,
};

#[derive(Clone, Copy, Debug)]
pub struct OpticalConfig {
    /// Fractions of physical capsule height (dimensionless).
    pub bevel: f32,
    pub refraction: f32,
    pub narrow_mix: f32,
    pub protection: [f32; 2],
    /// Center and width measured at a 26px capsule, multiplied by height / 26.
    pub reference_height: f32,
    pub outer_rim: [f32; 2],
    pub inner_rim: [f32; 2],
    /// Unit-ish light direction retained exactly from the V2.2 baseline.
    pub light_direction: [f32; 2],
    /// Normalized x center/spread; baseline and amplitude of the reflection arc.
    pub arc: [f32; 4],
    /// Interior inset fractions; normalized surface highlight position/spread.
    pub surface_inset: [f32; 2],
    pub surface_position: [f32; 2],
    pub surface_spread: [f32; 2],
    pub underside: [f32; 2],
}
pub const OPTICS: OpticalConfig = OpticalConfig {
    bevel: 0.12, refraction: 0.065, narrow_mix: 0.52,
    protection: [0.025, 0.13], reference_height: 26.0,
    outer_rim: [0.50, 0.35], inner_rim: [1.15, 0.38],
    light_direction: [-0.35, -0.93675], arc: [0.28, 0.42, 0.30, 0.70],
    surface_inset: [0.08, 0.20], surface_position: [0.30, 0.22],
    surface_spread: [0.52, 0.28], underside: [0.80, 0.20],
};

#[derive(Clone, Copy, Debug)]
pub struct ReadabilityConfig {
    /// Reserved until stages 2/4. Neutral values must NOT change stage-1 output.
    /// These mix material colors, never window or output alpha.
    pub veil_rgb: [f32; 3],
    pub veil_mix: f32,
    pub center_lift: f32,
    pub saturation: f32,
    pub local_contrast: f32,
    pub detail_suppression: f32,
}
#[derive(Clone, Copy, Debug)]
pub struct ThemeConfig {
    /// [edge intercept, edge slope, protected intercept, protected slope].
    /// Keep this affine tone/tint model; do not stack a second RGBA tint over it.
    pub luminance: [f32; 4],
    pub chroma: [f32; 2],
    pub tint_bias: [f32; 3],
    /// [inner shadow, primary outer reflection, opposite reflection, inner lift].
    pub rim: [f32; 4],
    /// [underside multiplication strength, smooth reflection addition].
    pub surface: [f32; 2],
    pub foreground: [f32; 3],
    pub readability: ReadabilityConfig,
}
const NEUTRAL: ReadabilityConfig = ReadabilityConfig {
    veil_rgb: [1.0, 1.0, 1.0], veil_mix: 0.0, center_lift: 0.0,
    saturation: 1.0, local_contrast: 1.0, detail_suppression: 0.0,
};
pub const LIGHT: ThemeConfig = ThemeConfig {
    luminance: [0.70, 0.24, 0.84, 0.105], chroma: [0.40, 0.24],
    tint_bias: [-0.002, 0.0, 0.004], rim: [0.05, 0.70, 0.025, 0.006],
    surface: [0.006, 0.010], foreground: [0.012, 0.020, 0.035],
    readability: NEUTRAL,
};
pub const DARK: ThemeConfig = ThemeConfig {
    luminance: [0.014, 0.070, 0.013, 0.045], chroma: [0.11, 0.09],
    tint_bias: [-0.001, 0.0, 0.003], rim: [0.08, 0.17, 0.008, 0.004],
    surface: [0.025, 0.012], foreground: [0.95, 0.97, 1.0],
    readability: ReadabilityConfig { veil_rgb: [0.08, 0.08, 0.08], ..NEUTRAL },
};

fn in_range(values: &[f32], lo: f32, hi: f32) -> bool {
    values.iter().all(|v| v.is_finite() && *v >= lo && *v <= hi)
}
impl ThemeConfig {
    pub fn validate(&self) -> Result<(), &'static str> {
        let r = self.readability;
        if !in_range(&self.luminance, 0.0, 1.0)
            || self.luminance[0] + self.luminance[1] > 1.0
            || self.luminance[2] + self.luminance[3] > 1.0
            || !in_range(&self.chroma, 0.0, 1.0)
            || !in_range(&self.tint_bias, -0.05, 0.05)
            || !in_range(&self.rim, 0.0, 1.0)
            || !in_range(&self.surface, 0.0, 0.10)
            || !in_range(&self.foreground, 0.0, 1.0)
            || !in_range(&r.veil_rgb, 0.0, 1.0)
            || !in_range(&[r.veil_mix, r.detail_suppression, r.local_contrast, r.saturation], 0.0, 1.0)
            || !in_range(&[r.center_lift], 0.0, 0.10)
        { return Err("Invalid material parameter range or non-finite value"); }
        Ok(())
    }
}
pub fn validate() -> Result<(), &'static str> {
    LIGHT.validate()?; DARK.validate()?;
    if !in_range(&[BLUR.center_fraction, BLUR.edge_fraction], 0.001, 0.30)
        || BLUR.edge_fraction > BLUR.center_fraction
        || !in_range(&[BLUR.max_sigma_pixels], 0.01, 20.0)
        || !in_range(&[BLUR.padding_fraction], 0.1, 1.0)
        || !in_range(&[OPTICS.bevel, OPTICS.refraction], 0.001, 0.30)
        || !in_range(&[OPTICS.narrow_mix], 0.0, 1.0)
        || !in_range(&OPTICS.protection, 0.0, 0.5)
        || OPTICS.protection[0] >= OPTICS.protection[1]
        || !in_range(&[OPTICS.reference_height], 1.0, 120.0)
        || !in_range(&OPTICS.outer_rim, 0.01, 3.0)
        || !in_range(&OPTICS.inner_rim, 0.01, 3.0)
        || !in_range(&OPTICS.light_direction, -1.0, 1.0)
        || !in_range(&OPTICS.arc, 0.01, 1.0)
        || !in_range(&OPTICS.surface_inset, 0.0, 0.5)
        || OPTICS.surface_inset[0] >= OPTICS.surface_inset[1]
        || !in_range(&OPTICS.surface_position, 0.0, 1.0)
        || !in_range(&OPTICS.surface_spread, 0.01, 1.0)
        || !in_range(&OPTICS.underside, 0.01, 1.0)
    { return Err("Invalid optical or filter configuration"); }
    for h in 20..=120 {
        let support = (3.0 * (h as f32 * BLUR.center_fraction).min(BLUR.max_sigma_pixels)).ceil();
        if support > (h as f32 * BLUR.padding_fraction).ceil() {
            return Err("Filter support exceeds the captured ROI padding");
        }
    }
    Ok(())
}

/// Generate compile-time HLSL constants, not a second runtime parameter store.
/// Nine decimal places round-trip the f32 art values. No shader file I/O/includes,
/// additional GPU buffers, per-frame parsing, or color-space conversion is added.
pub fn hlsl_header() -> Result<String, &'static str> {
    validate()?;
    let mut out = String::from("// Generated from material_config.rs; linear-light values.\n");
    fn value(out: &mut String, name: &str, values: &[f32]) {
        let numbers = values.iter().map(|v| format!("{v:.9}")).collect::<Vec<_>>().join(", ");
        if values.len() == 1 {
            writeln!(out, "static const float cfg_{name} = {numbers};").unwrap();
        } else {
            let n = values.len();
            writeln!(out, "static const float{n} cfg_{name} = float{n}({numbers});").unwrap();
        }
    }
    value(&mut out, "max_sigma", &[BLUR.max_sigma_pixels]);
    value(&mut out, "bevel", &[OPTICS.bevel]);
    value(&mut out, "refraction", &[OPTICS.refraction]);
    value(&mut out, "narrow_mix", &[OPTICS.narrow_mix]);
    value(&mut out, "protection", &OPTICS.protection);
    value(&mut out, "reference_height", &[OPTICS.reference_height]);
    value(&mut out, "outer_rim", &OPTICS.outer_rim);
    value(&mut out, "inner_rim", &OPTICS.inner_rim);
    value(&mut out, "light_direction", &OPTICS.light_direction);
    value(&mut out, "arc", &OPTICS.arc);
    value(&mut out, "surface_inset", &OPTICS.surface_inset);
    value(&mut out, "surface_position", &OPTICS.surface_position);
    value(&mut out, "surface_spread", &OPTICS.surface_spread);
    value(&mut out, "underside", &OPTICS.underside);
    for (name, theme) in [("light", LIGHT), ("dark", DARK)] {
        value(&mut out, &format!("{name}_luminance"), &theme.luminance);
        value(&mut out, &format!("{name}_chroma"), &theme.chroma);
        value(&mut out, &format!("{name}_tint"), &theme.tint_bias);
        value(&mut out, &format!("{name}_rim"), &theme.rim);
        value(&mut out, &format!("{name}_surface"), &theme.surface);
        value(&mut out, &format!("{name}_foreground"), &theme.foreground);
        let r = theme.readability;
        value(&mut out, &format!("{name}_veil_rgb"), &r.veil_rgb);
        value(&mut out, &format!("{name}_readability"), &[r.veil_mix,r.center_lift,r.local_contrast,r.saturation]);
        value(&mut out, &format!("{name}_detail"), &[r.detail_suppression]);
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test] fn defaults_validate_and_filters_keep_the_roi() { validate().unwrap(); }
    #[test] fn both_themes_have_distinct_configurations() {
        assert_ne!(LIGHT.luminance, DARK.luminance);
        assert_ne!(LIGHT.rim, DARK.rim);
        assert_ne!(LIGHT.foreground, DARK.foreground);
    }
    #[test] fn invalid_values_fail_before_compilation() {
        for invalid in [f32::NAN, f32::INFINITY, -0.01, 1.01] {
            let mut c = LIGHT; c.readability.veil_mix = invalid;
            assert!(c.validate().is_err());
        }
        let mut c = DARK; c.luminance[3] = 1.0;
        assert!(c.validate().is_err());
    }
    #[test] fn generated_numbers_are_finite_locale_independent_hlsl() {
        let header = hlsl_header().unwrap();
        assert!(header.contains("cfg_light_luminance = float4("));
        assert!(header.contains("cfg_dark_readability = float4("));
        assert!(!header.contains("NaN") && !header.contains("inf"));
        assert_eq!(header, hlsl_header().unwrap());
    }
}
