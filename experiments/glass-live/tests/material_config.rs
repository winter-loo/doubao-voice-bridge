//! Compile and test the parameter module on every host, before GPU integration.
//! The production shader is intentionally unchanged in this first checkpoint.
#[path = "../src/material_config.rs"]
mod material_config;

#[test]
fn stage_one_readability_controls_are_neutral() {
    for theme in [material_config::LIGHT, material_config::DARK] {
        assert_eq!(theme.readability.veil_mix, 0.0);
        assert_eq!(theme.readability.center_lift, 0.0);
        assert_eq!(theme.readability.detail_suppression, 0.0);
        assert_eq!(theme.readability.local_contrast, 1.0);
        assert_eq!(theme.readability.saturation, 1.0);
    }
}

#[test]
fn compact_filter_scales_are_recorded_in_physical_pixels() {
    let config = material_config::BLUR;
    assert!((39.0 * config.center_fraction - 7.8).abs() < 0.0001);
    assert!((39.0 * config.edge_fraction - 1.755).abs() < 0.0001);
    assert_eq!((39.0 * config.padding_fraction).ceil() as u32, 26);
}
