use std::{fs::{self, OpenOptions}, io::Write, path::{Path, PathBuf}};
use glass_material_fixture::{Capsule, Image, Parameters, Theme, composite, render};

fn write_new(path: &Path, bytes: &[u8]) -> Result<(), Box<dyn std::error::Error>> {
    let mut file = OpenOptions::new().create_new(true).write(true).open(path)?;
    file.write_all(bytes)?; Ok(())
}
fn write_ppm(path: &Path, image: &Image) -> Result<(), Box<dyn std::error::Error>> {
    let mut bytes = format!("P6\n{} {}\n255\n", image.width, image.height).into_bytes();
    bytes.extend(image.rgb8()); write_new(path, &bytes)
}
fn token<'a>(bytes: &'a [u8], cursor: &mut usize) -> Result<&'a str, String> {
    loop {
        while *cursor < bytes.len() && bytes[*cursor].is_ascii_whitespace() { *cursor += 1; }
        if bytes.get(*cursor) == Some(&b'#') {
            while *cursor < bytes.len() && bytes[*cursor] != b'\n' { *cursor += 1; }
        } else { break; }
    }
    let start = *cursor;
    while *cursor < bytes.len() && !bytes[*cursor].is_ascii_whitespace() { *cursor += 1; }
    if start == *cursor { return Err("missing PPM header token".into()); }
    std::str::from_utf8(&bytes[start..*cursor]).map_err(|_| "invalid PPM header".into())
}
fn decode_ppm(bytes: &[u8]) -> Result<Image, String> {
    let mut cursor = 0;
    if token(bytes, &mut cursor)? != "P6" { return Err("input must be an 8-bit binary P6 PPM".into()); }
    let width: u32 = token(bytes, &mut cursor)?.parse().map_err(|_| "bad PPM width")?;
    let height: u32 = token(bytes, &mut cursor)?.parse().map_err(|_| "bad PPM height")?;
    if token(bytes, &mut cursor)? != "255" { return Err("only maxval 255 is supported".into()); }
    // Consume exactly the header separator, NOT arbitrary whitespace: the first
    // real pixel may itself be 0x0a, 0x0d or 0x20.
    match bytes.get(cursor) {
        Some(b'\r') if bytes.get(cursor+1) == Some(&b'\n') => cursor += 2,
        Some(value) if value.is_ascii_whitespace() => cursor += 1,
        _ => return Err("missing PPM raster separator".into()),
    }
    Image::from_rgb8(width, height, &bytes[cursor..])
}
fn fixture(dark: bool) -> Image {
    let (w,h) = (640usize,200usize);
    let mut bytes = vec![if dark { 24 } else { 249 }; w*h*3];
    // Deliberately recognizable seven-segment digit strokes, not random noise.
    let digits: [u8;10] = [0x3f,0x06,0x5b,0x4f,0x66,0x6d,0x7d,0x07,0x7f,0x6f];
    let segments = [(3,0,15,3),(18,3,3,15),(18,21,3,15),(3,36,15,3),(0,21,3,15),(0,3,3,15),(3,18,15,3)];
    for row in 0..4 { for column in 0..23 {
        let digit = digits[(column+row*3)%10];
        for (bit, &(sx,sy,sw,sh)) in segments.iter().enumerate() {
            if digit & (1<<bit) == 0 { continue; }
            for y in (12+row*44+sy)..(12+row*44+sy+sh) { for x in (5+column*27+sx)..(5+column*27+sx+sw) {
                if x<w && y<h { let start=(y*w+x)*3; bytes[start..start+3].fill(if dark {210} else {12}); }
            } }
        }
    } }
    Image::from_rgb8(w as u32,h as u32,&bytes).unwrap()
}
fn run() -> Result<(), Box<dyn std::error::Error>> {
    let mut input: Option<PathBuf> = None; let mut output: Option<PathBuf> = None;
    for arg in std::env::args().skip(1) {
        if let Some(path) = arg.strip_prefix("--input=") { input = Some(PathBuf::from(path)); }
        else if let Some(path) = arg.strip_prefix("--output-dir=") { output = Some(PathBuf::from(path)); }
        else { return Err(format!("unknown argument {arg}; use --output-dir=NEW_DIRECTORY [--input=BACKGROUND.ppm]").into()); }
    }
    let output = output.ok_or("--output-dir is required (a NEW directory; no overwrite)")?;
    let loaded = if let Some(path) = input {
        if fs::metadata(&path)?.len() > 40 * 1024 * 1024 { return Err("input file too large".into()); }
        Some(decode_ppm(&fs::read(path)?)?)
    } else { None };
    // An existing directory is never reused or cleaned up.
    fs::create_dir(&output)?;
    for (name, theme) in [("light", Theme::Light), ("dark", Theme::Dark)] {
        let scene = loaded.clone().unwrap_or_else(|| fixture(matches!(theme, Theme::Dark)));
        if scene.width < 80 || scene.height < 40 { return Err("fixture must be at least 80x40".into()); }
        let height = (scene.height / 3).clamp(12, 80).min(scene.width / 4);
        let width = (height * 4).min(scene.width - 16);
        let capsule = Capsule { left: (scene.width-width)/2, top: (scene.height-height)/2, width, height };
        let layer = render(&scene, capsule, theme, Parameters::default())?;
        let composite = composite(&scene, capsule, &layer)?;
        write_ppm(&output.join(format!("{name}-background.ppm")), &scene)?;
        write_ppm(&output.join(format!("{name}-material.ppm")), &composite)?;
        let mut rgba = format!("P7\nWIDTH {}\nHEIGHT {}\nDEPTH 4\nMAXVAL 255\nTUPLTYPE RGB_ALPHA\nENDHDR\n", width, height).into_bytes();
        rgba.extend(layer.rgba8());
        write_new(&output.join(format!("{name}-layer.pam")), &rgba)?;
    }
    println!("Fixture render complete: {} (CPU reference; no desktop capture; not production GPU output)", output.display());
    Ok(())
}
fn main() { if let Err(error) = run() { eprintln!("glass fixture: {error}"); std::process::exit(1); } }

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn ppm_preserves_a_whitespace_valued_first_pixel() {
        let mut bytes = b"P6\n1 1\n255\n".to_vec(); bytes.extend([10,13,32]);
        assert_eq!(decode_ppm(&bytes).unwrap().rgb8(), [10,13,32]);
    }
    #[test]
    fn ppm_supports_comments_and_crlf() {
        let mut bytes=b"P6\r\n# fixture\r\n1 1\r\n255\r\n".to_vec(); bytes.extend([0,100,255]);
        assert_eq!(decode_ppm(&bytes).unwrap().rgb8(), [0,100,255]);
    }
    #[test]
    fn ppm_rejects_truncated_input() { assert!(decode_ppm(b"P6\n1 1\n255\n\x00").is_err()); }
}
