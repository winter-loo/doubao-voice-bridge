//! Issue #11: validated linear-light material parameters and frozen V2.2 profile.
//! Lengths are physical pixels or fractions of capsule size, never implicit DIPs.
//! Profiles are compile-time presets for ablation tests, not per-frame parsing.
use std::fmt::Write;

#[derive(Clone, Copy, Debug)]
pub struct BlurConfig {
    pub center_fraction: f32,
    pub edge_fraction: f32,
    pub max_sigma_pixels: f32,
    pub padding_fraction: f32,
}
pub const BLUR: BlurConfig = BlurConfig {
    center_fraction: 0.20, edge_fraction: 0.045,
    max_sigma_pixels: 20.0, padding_fraction: 0.90,
};
#[derive(Clone, Copy, Debug)]
pub struct OpticalConfig {
    pub bevel: f32,
    pub refraction: f32,
    pub narrow_mix: f32,
    pub protection: [f32; 2],
    /// Rim center/width at reference_height, scaled by physical height.
    pub reference_height: f32,
    pub outer_rim: [f32; 2],
    pub inner_rim: [f32; 2],
    pub light_direction: [f32; 2],
    pub arc: [f32; 4],
    pub surface_inset: [f32; 2],
    pub surface_position: [f32; 2],
    pub surface_spread: [f32; 2],
    pub underside: [f32; 2],
}
/// Immutable V2.2 optics; Candidate overrides named fields in optics().
pub const OPTICS: OpticalConfig = OpticalConfig {
    bevel: 0.12, refraction: 0.065, narrow_mix: 0.52,
    protection: [0.025, 0.13], reference_height: 26.0,
    outer_rim: [0.50, 0.35], inner_rim: [1.15, 0.38],
    light_direction: [-0.35, -0.93675], arc: [0.28, 0.42, 0.30, 0.70],
    surface_inset: [0.08, 0.20], surface_position: [0.30, 0.22],
    surface_spread: [0.52, 0.28], underside: [0.80, 0.20],
};
#[derive(Clone, Copy, Debug)]
pub struct ContentGeometry {
    /// Normalized center and full extent relative to the capsule rectangle.
    pub center: [f32; 2],
    pub extent: [f32; 2],
    /// Corner radius and inward soft transition, fractions of capsule height.
    pub corner: f32,
    pub feather: f32,
}
/// Slightly left-biased to include the waveform. 84% width, 76% height is the
/// outer support, not a uniform-strength patch; it includes a soft inner edge.
pub const CONTENT: ContentGeometry = ContentGeometry {
    center: [0.47, 0.50], extent: [0.84, 0.76], corner: 0.18, feather: 0.10,
};
#[derive(Clone, Copy, Debug)]
pub struct ReadabilityConfig {
    /// RGB and lift are LINEAR-light; mix never changes output/window alpha.
    pub veil_rgb: [f32; 3],
    pub veil_mix: f32,
    pub center_lift: f32,
    pub saturation: f32,
    /// Luminance residual relative to the existing wide blurred sample.
    pub local_contrast: f32,
    /// Attenuates narrow-minus-wide residual; not an additional blur sigma.
    pub detail_suppression: f32,
}
#[derive(Clone, Copy, Debug)]
pub struct ThemeConfig {
    /// [edge intercept, edge slope, protected intercept, protected slope].
    pub luminance: [f32; 4],
    pub chroma: [f32; 2],
    pub tint_bias: [f32; 3],
    /// [inner shadow, primary outer reflection, opposite reflection, inner lift].
    pub rim: [f32; 4],
    /// [underside multiplication, smooth reflection addition].
    pub surface: [f32; 2],
    pub foreground: [f32; 3],
    pub readability: ReadabilityConfig,
}
const NEUTRAL: ReadabilityConfig = ReadabilityConfig {
    veil_rgb: [1.0, 1.0, 1.0], veil_mix: 0.0, center_lift: 0.0,
    saturation: 1.0, local_contrast: 1.0, detail_suppression: 0.0,
};
/// Frozen stage-1 light/dark values, also used for the V2.2 parity contract.
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
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Profile { V22, Veil, Rim, Candidate }
pub fn theme(profile: Profile, dark: bool) -> ThemeConfig {
    let mut t = if dark { DARK } else { LIGHT };
    if profile != Profile::V22 {
        t.readability.veil_rgb = if dark { [0.060; 3] } else { [1.0; 3] };
        t.readability.veil_mix = if dark { 0.08 } else { 0.10 };
        t.readability.center_lift = if dark { 0.0015 } else { 0.0010 };
    }
    if matches!(profile,Profile::Rim|Profile::Candidate) {
        // Narrower, softer paired highlights. Existing V2.2 area-light sheen is
        // retained: stacking another white gradient would wash out the body.
        t.rim = if dark { [0.060,0.14,0.010,0.005] } else { [0.035,0.48,0.025,0.007] };
    }
    if profile==Profile::Candidate {
        t.readability.detail_suppression=if dark {0.44} else {0.50};
        t.readability.local_contrast=if dark {0.82} else {0.85};
        t.readability.saturation=if dark {0.94} else {0.97};
    }
    t
}
pub fn optics(profile: Profile) -> OpticalConfig {
    if matches!(profile,Profile::Rim|Profile::Candidate) {
        OpticalConfig {outer_rim:[0.50,0.30],inner_rim:[1.00,0.34],..OPTICS}
    } else {OPTICS}
}
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
    for profile in [Profile::V22, Profile::Veil, Profile::Rim, Profile::Candidate] {
        theme(profile, false).validate()?; theme(profile, true).validate()?;
        let o = optics(profile);
        if !in_range(&[o.bevel,o.refraction],0.001,0.30)
            || !in_range(&[o.narrow_mix],0.0,1.0)
            || !in_range(&o.protection,0.0,0.5) || o.protection[0]>=o.protection[1]
            || !in_range(&[o.reference_height],1.0,120.0)
            || !in_range(&o.outer_rim,0.01,3.0) || !in_range(&o.inner_rim,0.01,3.0)
            || !in_range(&o.light_direction,-1.0,1.0) || !in_range(&o.arc,0.01,1.0)
            || !in_range(&o.surface_inset,0.0,0.5) || o.surface_inset[0]>=o.surface_inset[1]
            || !in_range(&o.surface_position,0.0,1.0) || !in_range(&o.surface_spread,0.01,1.0)
            || !in_range(&o.underside,0.01,1.0)
        { return Err("Invalid optical configuration"); }
    }
    if !in_range(&[BLUR.center_fraction,BLUR.edge_fraction],0.001,0.30)
        || BLUR.edge_fraction>BLUR.center_fraction
        || !in_range(&[BLUR.max_sigma_pixels],0.01,20.0)
        || !in_range(&[BLUR.padding_fraction],0.1,1.0)
        || !in_range(&CONTENT.center,0.0,1.0) || !in_range(&CONTENT.extent,0.1,1.0)
        || !in_range(&[CONTENT.corner,CONTENT.feather],0.01,0.25)
    { return Err("Invalid filter or content geometry"); }
    for axis in 0..2 {
        if CONTENT.center[axis]-CONTENT.extent[axis]*0.5<0.0
            || CONTENT.center[axis]+CONTENT.extent[axis]*0.5>1.0 {
            return Err("Content support extends outside capsule rectangle");
        }
    }
    for h in 20..=120 {
        let support = (3.0*(h as f32*BLUR.center_fraction).min(BLUR.max_sigma_pixels)).ceil();
        if support>(h as f32*BLUR.padding_fraction).ceil() { return Err("Filter exceeds ROI padding"); }
    }
    Ok(())
}
/// Generate constants once at pipeline construction. The neutral profile is used
/// only by explicit tests; production and GPU self-test use Candidate.
pub fn hlsl_header() -> Result<String, &'static str> { hlsl_header_for(Profile::Candidate) }
pub fn hlsl_header_for(profile: Profile) -> Result<String, &'static str> {
    validate()?;
    let o = optics(profile);
    let mut out = String::from("// Generated validated linear-light parameters.\n");
    fn value(out: &mut String, name: &str, values: &[f32]) {
        let numbers=values.iter().map(|v|format!("{v:.9}")).collect::<Vec<_>>().join(", ");
        if values.len()==1 { writeln!(out,"static const float cfg_{name} = {numbers};").unwrap(); }
        else { let n=values.len(); writeln!(out,"static const float{n} cfg_{name} = float{n}({numbers});").unwrap(); }
    }
    value(&mut out,"max_sigma",&[BLUR.max_sigma_pixels]);
    value(&mut out,"bevel",&[o.bevel]); value(&mut out,"refraction",&[o.refraction]);
    value(&mut out,"narrow_mix",&[o.narrow_mix]); value(&mut out,"protection",&o.protection);
    value(&mut out,"reference_height",&[o.reference_height]);
    value(&mut out,"outer_rim",&o.outer_rim); value(&mut out,"inner_rim",&o.inner_rim);
    value(&mut out,"light_direction",&o.light_direction); value(&mut out,"arc",&o.arc);
    value(&mut out,"surface_inset",&o.surface_inset); value(&mut out,"surface_position",&o.surface_position);
    value(&mut out,"surface_spread",&o.surface_spread); value(&mut out,"underside",&o.underside);
    value(&mut out,"content_center",&CONTENT.center); value(&mut out,"content_extent",&CONTENT.extent);
    value(&mut out,"content_corner",&[CONTENT.corner]); value(&mut out,"content_feather",&[CONTENT.feather]);
    for (name,t) in [("light",theme(profile,false)),("dark",theme(profile,true))] {
        value(&mut out,&format!("{name}_luminance"),&t.luminance);
        value(&mut out,&format!("{name}_chroma"),&t.chroma);
        value(&mut out,&format!("{name}_tint"),&t.tint_bias);
        value(&mut out,&format!("{name}_rim"),&t.rim);
        value(&mut out,&format!("{name}_surface"),&t.surface);
        value(&mut out,&format!("{name}_foreground"),&t.foreground);
        let r=t.readability;
        value(&mut out,&format!("{name}_veil_rgb"),&r.veil_rgb);
        value(&mut out,&format!("{name}_readability"),&[r.veil_mix,r.center_lift,r.local_contrast,r.saturation]);
        value(&mut out,&format!("{name}_detail"),&[r.detail_suppression]);
    }
    Ok(out)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test] fn defaults_validate_and_filters_keep_the_roi() { validate().unwrap(); }
    #[test] fn both_themes_have_distinct_configurations() {
        assert_ne!(LIGHT.luminance,DARK.luminance); assert_ne!(LIGHT.foreground,DARK.foreground);
        assert_ne!(theme(Profile::Candidate,false).readability.veil_rgb,theme(Profile::Candidate,true).readability.veil_rgb);
    }
    #[test] fn invalid_values_fail_before_compilation() {
        for invalid in [f32::NAN,f32::INFINITY,-0.01,1.01] {
            let mut c=LIGHT; c.readability.veil_mix=invalid; assert!(c.validate().is_err());
        }
        let mut c=DARK; c.luminance[3]=1.0; assert!(c.validate().is_err());
    }
    #[test] fn generated_numbers_are_finite_locale_independent_hlsl() {
        let header=hlsl_header().unwrap();
        assert!(header.contains("cfg_light_luminance = float4("));
        assert!(header.contains("cfg_dark_readability = float4("));
        assert!(!header.contains("NaN")&&!header.contains("inf"));
        assert_eq!(header,hlsl_header().unwrap());
        assert_ne!(header,hlsl_header_for(Profile::V22).unwrap());
    }
    #[test] fn staged_profiles_do_not_duplicate_layers_or_change_foreground() {
        for dark in [false,true] {
            let a=theme(Profile::V22,dark); let b=theme(Profile::Veil,dark);
            let c=theme(Profile::Rim,dark); let d=theme(Profile::Candidate,dark);
            assert_eq!(a.readability.veil_mix,0.0); assert!(b.readability.veil_mix>0.0);
            assert_eq!(b.rim,a.rim); assert_ne!(c.rim,b.rim);
            assert_eq!(c.readability.detail_suppression,0.0);
            assert!(d.readability.detail_suppression>0.0 && d.readability.detail_suppression<1.0);
            assert_eq!(a.foreground,d.foreground); assert_eq!(a.surface,d.surface);
            assert!(optics(Profile::Rim).outer_rim[1]<OPTICS.outer_rim[1]);
        }
    }
}
