//! Frozen stage-1 parity plus independently exercised runtime material tests.
//! glass.hlsl stays byte-identical to 3974edb. No user pixels/windows/capture.
use super::*;

pub(super) unsafe fn reference_pipeline(device: ID3D11Device, context: ID3D11DeviceContext,
                             w: u32, h: u32, mask: &[u8]) -> AppResult<Pipeline> {
    let mut p = Pipeline::new(device, context, w, h, mask)?;
    let source = include_str!("glass.hlsl");
    let code = compile_source(source, s!("fullscreen_vs"), s!("vs_5_0"))?;
    let mut vertex = None;
    p.device.CreateVertexShader(blob_bytes(&code), None, Some(&mut vertex))?;
    p.vertex = vertex.ok_or("Missing baseline vertex shader")?;
    for (entry, slot) in [(s!("convert_ps"), &mut p.convert),
                         (s!("blur_ps"), &mut p.blur),
                         (s!("material_ps"), &mut p.material)] {
        let code = compile_source(source, entry, s!("ps_5_0"))?;
        let mut shader = None;
        p.device.CreatePixelShader(blob_bytes(&code), None, Some(&mut shader))?;
        *slot = shader.ok_or("Missing baseline pixel shader")?;
    }
    Ok(p)
}
unsafe fn configured_pipeline(device: ID3D11Device, context: ID3D11DeviceContext,
    w: u32, h: u32, mask: &[u8], profile: material_config::Profile) -> AppResult<Pipeline> {
    let mut p = Pipeline::new(device,context,w,h,mask)?;
    let source = format!("{}\n{}\n{}",material_config::hlsl_header_for(profile)?,
        include_str!("glass.hlsl"),include_str!("configured_material.hlsl"));
    let code=compile_source(&source,s!("configured_material_ps"),s!("ps_5_0"))?;
    let mut shader=None;
    p.device.CreatePixelShader(blob_bytes(&code),None,Some(&mut shader))?;
    p.material=shader.ok_or("Missing configured test shader")?;
    Ok(p)
}
fn input(w: u32, h: u32, kind: u32, offset: u32) -> Vec<u8> {
    let mut pixels = Vec::with_capacity((w*h*4) as usize);
    for y in 0..h { for x in 0..w {
        let x = x + offset;
        let c = match kind {
            0 => [0,0,0,255],
            1 => [128,128,128,255],
            2 => [255,255,255,255],
            3 => if (x/3+y/11)%2==0 {[0,0,0,255]} else {[255,255,255,255]},
            4 => if x<w/2 {[64,48,220,255]} else {[224,112,32,255]},
            _ => if x%17<3 || (x%17<12 && matches!(y%24, 3..=5|11..=13|19..=21))
                 {[15,15,15,255]} else {[249,249,249,255]},
        };
        pixels.extend_from_slice(&c);
    } }
    pixels
}
unsafe fn upload(p: &Pipeline, pixels: &[u8], reference: bool) {
    p.context.UpdateSubresource(&p.raw.texture,0,None,pixels.as_ptr().cast(),p.raw.width*4,0);
    if reference { p.prepare_scales(0.20,0.045); } else { p.prepare(); }
}
#[test]
fn parameter_refactor_matches_v22_on_gpu() -> AppResult<()> {
    unsafe {
        let (device, context) = create_device(None)?;
        let mut max_rgb_error = 0u8;
        let mut compared_pixels = 0usize;
        let mut cases = 0usize;
        let dir = std::env::var_os("GLASS_MATERIAL_FIXTURES")
            .map(|p| std::path::PathBuf::from(p).with_extension("issue11"));
        if let Some(dir) = &dir { std::fs::create_dir_all(dir)?; }
        for (w,h) in [(108u32,26u32),(162,39),(324,78)] {
            let mask: Vec<u8> = (0..w*h).map(|i| {
                let (x,y) = (i%w,i/w);
                if y>h/3 && y<h*2/3 && x>h && x<w-h { match x%7 {0=>255,1=>128,_=>0} } else {0}
            }).collect();
            // This test isolates the neutral profile explicitly. Current runtime
            // defaults have their own contracts below; they are NOT called equal.
            let current = configured_pipeline(device.clone(),context.clone(),w,h,&mask,material_config::Profile::V22)?;
            let baseline = reference_pipeline(device.clone(),context.clone(),w,h,&mask)?;
            assert_eq!(current.padding,(h as f32*0.65).ceil() as u32);
            assert_eq!((current.raw.width,current.raw.height),(baseline.raw.width,baseline.raw.height));
            for kind in 0..=5 {
                for offset in if kind==5 { &[0u32,1,7][..] } else { &[0u32][..] } {
                    let pixels = input(current.raw.width,current.raw.height,kind,*offset);
                    upload(&current,&pixels,false); upload(&baseline,&pixels,true);
                    for dark in [false,true] {
                        for foreground in [false,true] {
                            for phase in [0.0,1.25] {
                                current.render(dark,phase,foreground); baseline.render(dark,phase,foreground);
                                let a = baseline.read_rgba(&baseline.output)?;
                                let b = current.read_rgba(&current.output)?;
                                for (a,b) in a.chunks_exact(4).zip(b.chunks_exact(4)) {
                                    assert_eq!(a[3],b[3],"Silhouette changed during parameter refactor");
                                    for k in 0..3 {
                                        max_rgb_error = max_rgb_error.max(a[k].abs_diff(b[k]));
                                        assert!(a[k].abs_diff(b[k])<=1,
                                            "V2.2 refactor mismatch: size={w}x{h} kind={kind} offset={offset} dark={dark} fg={foreground}: {a:?} -> {b:?}");
                                        assert!(b[k]<=b[3],"Invalid premultiplied output");
                                    }
                                    compared_pixels += 1;
                                }
                                cases += 1;
                            }
                        }
                        if w==162 && *offset==0 && (kind==4 || kind==5) {
                            if let Some(dir) = &dir {
                                let dir = dir.join(format!("{}-{}",if kind==4 {"colors"} else {"text"},if dark {"dark"} else {"light"}));
                                std::fs::create_dir_all(&dir)?;
                                baseline.snapshot(&dir,1)?; current.snapshot(&dir,2)?;
                            }
                        }
                    }
                }
            }
        }
        let metrics = format!("{{\"stage\":1,\"scope\":\"synthetic WARP; neutral configured profile versus retained V2.2, NOT current visual acceptance\",\"baseline_commit\":\"3974edb0c7fbd0585e5f2e383b6669c9a4835288\",\"cases\":{cases},\"compared_pixels\":{compared_pixels},\"max_rgb_error\":{max_rgb_error},\"alpha_exact\":true}}");
        eprintln!("[issue11-refactor] {metrics}");
        if let Some(dir) = &dir { std::fs::write(dir.join("metrics.json"),metrics)?; }
        Ok(())
    }
}

#[path = "v23_tests.rs"]
mod v23_tests;
