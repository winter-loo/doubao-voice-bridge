//! Offscreen WARP tests. All input pixels below are generated here, never captured.
//! V1 uses its original blur scales; the pinned V2 baseline uses the same filters.
//! Historical V2.1 rim checks run with ONLY the new surface reflection disabled.
//! Final V2.2 output also has independent contrast, foreground and relief checks.
//! These are regression bounds, not OCR scores, display timing or Apple parity.
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
    crate::png::write(&directory.join(name), pipe.raw.width, pipe.raw.height, &rgba)?;
    Ok(())
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
            // Sample the unchanged thin rim at its actual 1.5px inset.
            let rim = row_std(&v2, 1);
            assert!(before > 0.5, "The baseline fixture must actually contain measurable variation");
            assert!(after < before * 0.65 + 0.15, "Center suppression regressed: V1={before}, current={after}");
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
        // These original bounds also apply to the FINAL material with reflection.
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
        let summary = format!("{{\"scope\":\"synthetic WARP pixels; no desktop capture or display timing; V1 uses original shader and blur scales\",\"material\":\"V2.2\",\"rim_probe_inset_pixels\":1.5,\"width\":{rw},\"height\":{rh},\"metrics\":[{}]}}", metrics.join(","));
        eprintln!("[material-v2] {summary}");
        if let Some(dir) = &directory { std::fs::write(dir.join("metrics.json"), summary)?; }
        let no_reflection = reflection_control(&current, &mask)?;
        // Retain the exact historical center/rim regression on the transmission
        // stage, then independently bound the final new reflection below.
        rim_refinement_contract(&no_reflection, &mask, directory.as_deref())?;
        surface_reflection_contract(&current, &no_reflection, &mask, directory.as_deref())?;
        Ok(())
    }
}

fn flat_row_mean(pixels: &[u8], y: usize) -> f64 {
    (H..W-H).map(|x| luminance(pixel(pixels,x,y))).sum::<f64>() / (W-2*H) as f64
}
fn broad_lip_rows(pixels: &[u8]) -> usize {
    let body = flat_row_mean(pixels,H/2);
    (1..H/2).filter(|&y| (flat_row_mean(pixels,y)-body).abs() > 2.0).count()
}
unsafe fn rim_refinement_contract(current: &Pipeline, mask: &[u8], directory: Option<&Path>) -> AppResult<()> {
    let mut old = Pipeline::new(current.device.clone(),current.context.clone(),W as u32,H as u32,mask)?;
    let source = format!("{}\n{}",include_str!("glass.hlsl"),include_str!("material_v2_test.hlsl"));
    let code = compile_source(&source,s!("v2_material_ps"),s!("ps_5_0"))?;
    let mut shader = None;
    old.device.CreatePixelShader(blob_bytes(&code),None,Some(&mut shader))?;
    old.material = shader.ok_or("Missing pinned V2 shader")?;
    let (rw,rh) = (current.raw.width,current.raw.height);
    let dir = directory.map(|p|p.join("rim-refinement"));
    if let Some(dir) = &dir { std::fs::create_dir(dir)?; }
    let mut metrics = Vec::new();
    for value in [0,128,255] {
        let pixels = fixture(rw,rh,|_,_|[value,value,value,255]);
        upload(&old,&pixels,false); upload(current,&pixels,false);
        for dark in [false,true] {
            old.render(dark,PHASE,false); current.render(dark,PHASE,false);
            let a = old.read_rgba(&old.output)?; let b = current.read_rgba(&current.output)?;
            let before = broad_lip_rows(&a); let after = broad_lip_rows(&b);
            assert!(after <= before,"Rim widened: value={value}, dark={dark}, V2={before}, V2.1={after}");
            if (value == 0 && !dark) || (value == 255 && dark) {
                assert!(before >= 6 && after <= before/2,"Broad lip not sufficiently reduced: {before} -> {after}");
            }
            let body = flat_row_mean(&b,H/2);
            for y in 6..H/2 {
                assert!((flat_row_mean(&b,y)-body).abs() <= 1.,"New lighting reaches beyond the thin rim");
            }
            metrics.push(format!("{{\"background\":{value},\"dark\":{dark},\"lip_rows_v2\":{before},\"lip_rows_v21\":{after}}}"));
        }
    }
    let text = text_fixture(rw,rh);
    let colors = fixture(rw,rh,|x,_|if x<rw/2{[64,48,220,255]}else{[224,112,32,255]});
    for (name,pixels) in [("text",text),("colors",colors)] {
        upload(&old,&pixels,false); upload(current,&pixels,false);
        for dark in [false,true] {
            old.render(dark,PHASE,true); current.render(dark,PHASE,true);
            let a = old.read_rgba(&old.output)?; let b = current.read_rgba(&current.output)?;
            let mut protected = 0usize; let mut altered = 0usize;
            for y in 0..H { for x in 0..W {
                let pa = pixel(&a,x,y); let pb = pixel(&b,x,y);
                assert_eq!(pa[3],pb[3],"V2 silhouette or alpha changed");
                let qx = x as f32+0.5-W as f32*0.5;
                let qy = y as f32+0.5-H as f32*0.5;
                let half = (W-H) as f32*0.5;
                let inset = H as f32*0.5-((qx-qx.clamp(-half,half)).powi(2)+qy*qy).sqrt();
                if inset >= H as f32*0.23+1. {
                    protected += 1;
                    assert!(pa.iter().zip(pb).all(|(a,b)|a.abs_diff(*b)<=1),"Protected V2 body changed");
                }
                if mask[y*W+x] == 255 { assert_eq!(pa,pb,"Foreground changed"); }
                if pa[..3].iter().zip(&pb[..3]).any(|(a,b)|a.abs_diff(*b)>2) { altered+=1; }
            } }
            assert!(protected>1000 && altered>100,"Invalid test coverage or ineffective edge change");
            if let Some(dir) = &dir {
                let theme = if dark{"dark"}else{"light"};
                export(&old,dir,&format!("{name}-{theme}-v2.png"))?;
                export(current,dir,&format!("{name}-{theme}-v21.png"))?;
            }
        }
    }
    let summary = format!("{{\"scope\":\"synthetic WARP, pinned V2 versus V2.1 transmission; new reflection disabled only for this historical contract; no user pixels\",\"metrics\":[{}]}}",metrics.join(","));
    eprintln!("[material-v21] {summary}");
    if let Some(dir) = &dir { std::fs::write(dir.join("metrics.json"),summary)?; }
    Ok(())
}

unsafe fn reflection_control(current: &Pipeline, mask: &[u8]) -> AppResult<Pipeline> {
    let mut pipe = Pipeline::new(current.device.clone(),current.context.clone(),W as u32,H as u32,mask)?;
    let source = format!("#define GLASS_SURFACE_REFLECTION_TEST_OFF 1\n{}",include_str!("glass.hlsl"));
    let code = compile_source(&source,s!("material_ps"),s!("ps_5_0"))?;
    let mut shader = None;
    pipe.device.CreatePixelShader(blob_bytes(&code),None,Some(&mut shader))?;
    pipe.material = shader.ok_or("Missing reflection-off control shader")?;
    Ok(pipe)
}
fn inset_at(x: usize, y: usize) -> f32 {
    let qx = x as f32+0.5-W as f32*0.5;
    let qy = y as f32+0.5-H as f32*0.5;
    let half = (W-H) as f32*0.5;
    H as f32*0.5-((qx-qx.clamp(-half,half)).powi(2)+qy*qy).sqrt()
}
fn relative_luminance(p: &[u8]) -> f64 {
    let linear = |v: u8| { let c = v as f64/255.; if c <= 0.04045 {c/12.92} else {((c+0.055)/1.055).powf(2.4)} };
    0.2126*linear(p[0])+0.7152*linear(p[1])+0.0722*linear(p[2])
}
fn center_difference(a: &[u8], b: &[u8]) -> f64 {
    let mut sum = 0.; let mut count = 0;
    for y in H/3..H*2/3 { for x in H..W-H {
        sum += (luminance(pixel(a,x,y))-luminance(pixel(b,x,y))).abs(); count += 1;
    } }
    sum/count as f64
}
unsafe fn surface_reflection_contract(current: &Pipeline, control: &Pipeline, mask: &[u8], directory: Option<&Path>) -> AppResult<()> {
    let (rw,rh) = (current.raw.width,current.raw.height);
    let dir = directory.map(|p|p.join("surface-reflection"));
    if let Some(dir) = &dir { std::fs::create_dir(dir)?; }
    let mut metrics = Vec::new();
    for value in [0,128,255] {
        let input = fixture(rw,rh,|_,_|[value,value,value,255]);
        upload(current,&input,false); upload(control,&input,false);
        for dark in [false,true] {
            control.render(dark,PHASE,false); current.render(dark,PHASE,false);
            let before = control.read_rgba(&control.output)?;
            let after = current.read_rgba(&current.output)?;
            let mut max_shift = 0u8;
            for y in 0..H { for x in 0..W {
                let a = pixel(&before,x,y); let b = pixel(&after,x,y);
                assert_eq!(a[3],b[3],"Reflection changed coverage");
                assert!(b[..3].iter().all(|v|*v<=b[3]),"Reflection broke premultiplication");
                for k in 0..3 { max_shift = max_shift.max(a[k].abs_diff(b[k])); }
                if inset_at(x,y) <= H as f32*0.08 {
                    assert!(a.iter().zip(b).all(|(a,b)|a.abs_diff(*b)<=1),"Thin rim widened by reflection");
                }
            } }
            assert!(max_shift <= if dark {16} else {3},"Reflection too strong: dark={dark}, max={max_shift}");
            let x = (W as f32*0.30) as usize;
            let y = (H as f32*0.28) as usize;
            let top_lift = luminance(pixel(&after,x,y))-luminance(pixel(&before,x,y));
            let bottom_lift = luminance(pixel(&after,x,H-1-y))-luminance(pixel(&before,x,H-1-y));
            assert!(top_lift > if dark {3.0} else {0.3},"Reflection disappeared: {top_lift}");
            assert!(top_lift > bottom_lift+0.3,"Reflection became a uniform overlay");
            metrics.push(format!("{{\"background\":{value},\"dark\":{dark},\"max_rgb_shift\":{max_shift},\"top_lift\":{top_lift:.4},\"bottom_lift\":{bottom_lift:.4}}}"));
        }
    }
    // Invert the same stripes to measure INPUT-dependent contrast, cancelling the
    // static reflection. A smooth gradient must not conceal sharper background.
    let stripes_a = fixture(rw,rh,|x,_|{let v=if x/8%2==0{0}else{255};[v,v,v,255]});
    let stripes_b = fixture(rw,rh,|x,_|{let v=if x/8%2==0{255}else{0};[v,v,v,255]});
    for dark in [false,true] {
        upload(current,&stripes_a,false); upload(control,&stripes_a,false);
        current.render(dark,PHASE,false); control.render(dark,PHASE,false);
        let a = current.read_rgba(&current.output)?; let old_a = control.read_rgba(&control.output)?;
        upload(current,&stripes_b,false); upload(control,&stripes_b,false);
        current.render(dark,PHASE,false); control.render(dark,PHASE,false);
        let b = current.read_rgba(&current.output)?; let old_b = control.read_rgba(&control.output)?;
        let response = center_difference(&a,&b); let old_response = center_difference(&old_a,&old_b);
        assert!(response <= old_response+0.5,"Reflection amplified background contrast: {old_response} -> {response}");
        metrics.push(format!("{{\"dark\":{dark},\"stripe_response_v21\":{old_response:.6},\"stripe_response_v22\":{response:.6}}}"));
    }
    let text = text_fixture(rw,rh);
    let colors = fixture(rw,rh,|x,_|if x<rw/2{[64,48,220,255]}else{[224,112,32,255]});
    let white = fixture(rw,rh,|_,_|[255,255,255,255]);
    for (name,input) in [("text",text),("colors",colors),("white",white)] {
        upload(current,&input,false); upload(control,&input,false);
        for dark in [false,true] {
            current.render(dark,PHASE,false);
            let background = current.read_rgba(&current.output)?;
            current.render(dark,PHASE,true); control.render(dark,PHASE,true);
            let after = current.read_rgba(&current.output)?;
            let before = control.read_rgba(&control.output)?;
            for (i,m) in mask.iter().enumerate() {
                if *m == 255 {
                    let fg = &after[i*4..i*4+4]; let bg = &background[i*4..i*4+4];
                    assert_eq!(fg,&before[i*4..i*4+4],"Reflection altered opaque foreground");
                    let a = relative_luminance(fg); let b = relative_luminance(bg);
                    assert!((a.max(b)+0.05)/(a.min(b)+0.05) >= 7.0,"Synthetic foreground contrast fell below design bound");
                }
            }
            if name == "colors" {
                let a = pixel(&background,30,H/2); let b = pixel(&background,W-30,H/2);
                assert!(a[0] > a[2]+5 && b[2] > b[0]+5,"Background color response disappeared");
            }
            if let Some(dir) = &dir {
                let theme = if dark{"dark"}else{"light"};
                export(control,dir,&format!("{name}-{theme}-v21.png"))?;
                export(current,dir,&format!("{name}-{theme}-v22.png"))?;
            }
        }
    }
    let summary = format!("{{\"scope\":\"synthetic WARP; same filters/input/phase; V2.1 transmission versus final V2.2 surface reflection; not user screenshot or display acceptance\",\"metrics\":[{}]}}",metrics.join(","));
    eprintln!("[material-v22] {summary}");
    if let Some(dir) = &dir { std::fs::write(dir.join("metrics.json"),summary)?; }
    Ok(())
}
