use std::{io::Cursor, sync::OnceLock};

pub const VOICE_T_TRAY_ICON_SIZE: u32 = 24;

const TRAY_ICON_PNG: &[u8] = include_bytes!("../assets/doubao-voice-tray-template-24.png");
const CORE_BLUE: [u8; 3] = [24, 121, 255];
static TRAY_ICON_ARGB: OnceLock<Vec<u8>> = OnceLock::new();

pub fn voice_t_tray_icon_argb() -> Vec<u8> {
    TRAY_ICON_ARGB.get_or_init(decode_tray_icon_argb).clone()
}

fn decode_tray_icon_argb() -> Vec<u8> {
    let decoder = png::Decoder::new(Cursor::new(TRAY_ICON_PNG));
    let mut reader = decoder
        .read_info()
        .expect("embedded Voice T tray template must be a valid PNG");
    let output_size = reader
        .output_buffer_size()
        .expect("embedded Voice T tray template must fit the PNG decoder limits");
    let mut rgba = vec![0; output_size];
    let info = reader
        .next_frame(&mut rgba)
        .expect("embedded Voice T tray template must decode");

    assert_eq!(info.width, VOICE_T_TRAY_ICON_SIZE);
    assert_eq!(info.height, VOICE_T_TRAY_ICON_SIZE);
    assert_eq!(info.color_type, png::ColorType::Rgba);
    assert_eq!(info.bit_depth, png::BitDepth::Eight);

    rgba[..info.buffer_size()]
        .chunks_exact(4)
        .flat_map(|pixel| [pixel[3], CORE_BLUE[0], CORE_BLUE[1], CORE_BLUE[2]])
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn canonical_core_blue() -> [u8; 3] {
        let document: serde_json::Value =
            serde_json::from_str(include_str!("../../../branding/voice-t/brand-colors.json"))
                .expect("canonical Voice T brand colors must be valid JSON");
        let rgb = document["colors"]["core_blue"]["rgb"]
            .as_array()
            .expect("canonical core blue must define an RGB array");

        [
            rgb[0].as_u64().expect("red must be an integer") as u8,
            rgb[1].as_u64().expect("green must be an integer") as u8,
            rgb[2].as_u64().expect("blue must be an integer") as u8,
        ]
    }

    #[test]
    fn voice_t_template_decodes_to_brand_blue_argb() {
        let pixels = voice_t_tray_icon_argb();

        assert_eq!(
            pixels.len(),
            (VOICE_T_TRAY_ICON_SIZE * VOICE_T_TRAY_ICON_SIZE * 4) as usize
        );
        assert_eq!(CORE_BLUE, canonical_core_blue());
        assert!(
            pixels
                .chunks_exact(4)
                .any(|pixel| { pixel == [255, CORE_BLUE[0], CORE_BLUE[1], CORE_BLUE[2]] })
        );
        assert!(pixels.chunks_exact(4).any(|pixel| pixel[0] == 0));
    }
}
