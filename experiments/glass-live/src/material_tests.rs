//! Offscreen WARP tests. All input pixels below are generated here, never captured.
//! The baseline uses the original material AND original blur scales. These tests
//! are regression bounds, not OCR scores, display performance or Apple parity.
use super::*;

const W: usize = 162;
const H: usize = 39;
const PHASE: f32 = 1.25;

unsafe fn baseline(device: ID3D11Device, context: ID3D11DeviceContext, mask: &[u8]) -> AppResult<Pipeline> {
    let mut pipe = Pipeline::new(device, context, W as u32, H as u32, mask)?;
    let source = format!("{}\n{}", include_str!("glass.hlsl"), include_str!("material_v1_test.hlsl"));
    let bytecode = compile_source(&source, s!("baseline_ps"), s!("ps_5_0"))?;
    let mut shader = None;
    pipe.device.CreatePixelShader(blob_bytes(&bytecode), None, Some(&mut shader))?;
    pipe.material = shader.ok_or("Missing baseline shader")?;
    Ok(pipe)
}
unsafe fn upload(pipe: &Pipeline, pixels: &[u8], v1: bool) {
    assert_eq!(pixels.len(), (pipe.raw.width * pipe.raw.height * 4) as usize);
    pipe.context.UpdateSubresource(&pipe.raw.texture, 0, None, pixels.as_ptr().cast(), pipe.raw.width * 4, 0);
    if v1 { pipe.prepare_scales(0.16, 0.035); } else { pipe.prepare(); }
}
fn pixel(pixels: &[u8], x: usize, y: usize) -> &[u8] {
    &pixels[(y * W + x) * 4..][..4]
}
fn luminance(p: &[u8]) -> f64 {
    0.2126 * p[0] as f64 + 0.7152 * p[1] as f64 + 0.0722 * p[2] as f64
}
fn row_std(pixels: &[u8], y: usize) -> f64 {
    let v: Vec<f64> = (H..W-H).map(|x| luminance(pixel(pixels, x, y))).collect();
    let mean = v.iter().sum::<f64>() / v.len() as f64;
    (v.iter().map(|x| (x-mean).powi(2)).sum::<f64>() / v.len() as f64).sqrt()
}
fn fixture(width: u32, height: u32, f: impl Fn(u32, u32) -> [u8; 4]) -> Vec<u8> {
    let mut out = Vec::with_capacity((width * height * 4) as usize);
    for y in 0..height { for x in 0..width { out.extend_from_slice(&f(x, y)); } }
    out
}

// Small original block glyph fixtures. No external font or user screenshot asset.
const DIGITS: [[u8; 7]; 10] = [
    [14,17,19,21,25,17,14], [4,12,4,4,4,4,14], [14,17,1,2,4,8,31],
    [30,1,1,14,1,1,30], [2,6,10,18,31,2,2], [31,16,16,30,1,1,30],
    [14,16,16,30,17,17,14], [31,1,2,4,8,8,8], [14,17,17,14,17,17,14],
    [14,17,17,15,1,1,14],
];
fn foreground_mask() -> Vec<u8> {
    let glyphs = [
        [30,17,17,30,20,18,17], // R
        [31,16,16,30,16,16,31], // E
        [14,17,17,31,17,17,17], // A
        [30,17,17,17,17,17,30], // D
        DIGITS[1], DIGITS[2], DIGITS[3],
    ];
    let mut mask = vec![0; W*H];
    for (i, glyph) in glyphs.iter().enumerate() {
        for y in 0..14 { for x in 0..10 {
            if glyph[y/2] & (1 << (4-x/2)) != 0 { mask[(12+y)*W + 56+i*12+x] = 255; }
        } }
    }
    mask
}
fn text_fixture(width: u32, height: u32) -> Vec<u8> {
    fixture(width, height, |x, y| {
        let cell_x = x as usize % 18;
        let cell_y = (y as usize + 7) % 27;
        let glyph = &DIGITS[(x as usize / 18 + y as usize / 27) % 10];
        let ink = cell_x < 15 && cell_y < 21 && glyph[cell_y/3] & (1 << (4-cell_x/3)) != 0;
        if ink { [12,12,12,255] } else { [249,249,249,255] }
    })
}
unsafe fn export(pipe: &Pipeline, directory: &Path, name: &str) -> AppResult<()> {
    let mut rgba = pipe.read_rgba(&pipe.raw)?;
    let layer = pipe.read_rgba(&pipe.output)?;
    for y in 0..H { for x in 0..W {
        let a = (y*W+x)*4;
        let b = ((y+pipe.padding as usize)*pipe.raw.width as usize+x+pipe.padding as usize)*4;
        for k in 0..3 {
            rgba[b+k] = (layer[a+k] as u32 + (rgba[b+k] as u32 * (255-layer[a+3]) as u32+127)/255).min(255) as u8;
        }
    } }
    crate::png::write(&directory.join(name), pipe.raw.width, pipe.raw.height, &rgba)
}

#[test]
fn center_filter_support_stays_inside_existing_roi() {
    for h in 20..=120 {
        let padding = (h as f32 * 0.65).ceil() as u32;
        let support = (3.0 * (h as f32 * CENTER_SIGMA_FRACTION).min(20.0)).ceil() as u32;
        assert!(support <= padding, "height={h}");
    }
}

#[test]
fn spatial_material_gpu_contract() -> AppResult<()> {
    unsafe {
        let (device, context) = create_device(None)?;
        let mask = foreground_mask();
        let current = Pipeline::new(device.clone(), context.clone(), W as u32, H as u32, &mask)?;
        let previous = baseline(device, context, &mask)?;
        let (rw, rh) = (current.raw.width, current.raw.height);
        assert_eq!((rw, rh), (214, 91), "No enlarged live capture region in this tuning");
        let directory = std::env::var_os("GLASS_MATERIAL_FIXTURES").map(std::path::PathBuf::from);
        if let Some(dir) = &directory { std::fs::create_dir(dir)?; }
        let stripes = fixture(rw, rh, |x, _| { let v = if x/8%2 == 0 {0} else {255}; [v,v,v,255] });
        upload(&previous, &stripes, true);
        upload(&current, &stripes, false);
        let mut metrics = Vec::new();
        for dark in [false, true] {
            previous.render(dark, PHASE, false);
            current.render(dark, PHASE, false);
            let v1 = previous.read_rgba(&previous.output)?;
            let v2 = current.read_rgba(&current.output)?;
            let before = row_std(&v1, H/2);
            let after = row_std(&v2, H/2);
            let rim = row_std(&v2, 3);
            assert!(before > 0.5, "The baseline fixture must actually contain measurable variation");
            assert!(after < before * 0.65 + 0.15, "Center suppression regressed: V1={before}, V2={after}");
            assert!(rim > after + 1.0 && rim < 35.0, "Rim must transmit more structure, not become unfiltered: {rim}");
            for (a, b) in v1.chunks_exact(4).zip(v2.chunks_exact(4)) {
                assert_eq!(a[3], b[3], "Coverage must not change with material tuning");
                assert!(b[..3].iter().all(|c| *c <= b[3]), "Invalid premultiplication");
            }
            for y in H/3..H*2/3 { for x in H..W-H {
                assert_eq!(pixel(&v2, x, y)[3], 255, "Sharp-background alpha leak");
            } }
            metrics.push(format!("{{\"theme\":\"{}\",\"stripe_center_std_v1\":{before:.6},\"stripe_center_std_v2\":{after:.6},\"stripe_rim_std_v2\":{rim:.6}}}", if dark {"dark"} else {"light"}));
        }
        // Bounds over uniform black/white: clean pearl light body, darker charcoal
        // body, and a non-flat directional rim. No thresholds on average screenshot RGB.
        for value in [0, 255] {
            let solid = fixture(rw, rh, |_, _| [value,value,value,255]);
            upload(&current, &solid, false);
            for dark in [false,true] {
                current.render(dark, PHASE, false);
                let pixels = current.read_rgba(&current.output)?;
                let center = pixel(&pixels,W/2,H/2);
                if dark { assert!(center[..3].iter().all(|c| *c >= 20 && *c <= 80)); }
                else { assert!(center[..3].iter().all(|c| *c >= 230 && *c <= 251)); }
                let x = (W as f32*0.28) as usize;
                assert!(luminance(pixel(&pixels,x,1)) > luminance(pixel(&pixels,x,H-2)) + 1., "Directional rim disappeared");
            }
        }
        let colors = fixture(rw, rh, |x, _| if x < rw/2 {[64,48,220,255]} else {[224,112,32,255]});
        let text = text_fixture(rw, rh);
        for (name, pixels) in [("colors", colors), ("text", text)] {
            upload(&previous, &pixels, true);
            upload(&current, &pixels, false);
            for dark in [false,true] {
                let theme = if dark {"dark"} else {"light"};
                previous.render(dark, PHASE, true);
                current.render(dark, PHASE, true);
                let v1 = previous.read_rgba(&previous.output)?;
                let v2 = current.read_rgba(&current.output)?;
                for (i, m) in mask.iter().enumerate() {
                    if *m == 255 { assert_eq!(&v1[i*4..i*4+4], &v2[i*4..i*4+4], "Opaque foreground changed"); }
                }
                if let Some(dir) = &directory {
                    export(&previous, dir, &format!("{name}-{theme}-v1.png"))?;
                    export(&current, dir, &format!("{name}-{theme}-v2.png"))?;
                }
            }
        }
        let summary = format!("{{\"scope\":\"synthetic WARP pixels; no desktop capture or display timing; V1 uses original shader and blur scales\",\"width\":{rw},\"height\":{rh},\"metrics\":[{}]}}", metrics.join(","));
        eprintln!("[material-v2] {summary}");
        if let Some(dir) = &directory { std::fs::write(dir.join("metrics.json"), summary)?; }
        Ok(())
    }
}
