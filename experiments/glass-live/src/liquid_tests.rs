use super::*;
use windows::{core::w,Win32::{Foundation::COLORREF,Graphics::Gdi::*}};

fn source(w:u32,h:u32,kind:u32,shift:u32)->Vec<u8> {
    let mut bytes=Vec::with_capacity((w*h*4) as usize);
    for y in 0..h {for x in 0..w{
        let xx=x+shift;
        let c=match kind {
            0=>[0,0,0,255], 1=>[255,255,255,255], 2=>[232,232,232,255],
            3=>if xx%w<w/2{[64,48,220,255]}else{[224,112,32,255]},
            4=>if xx%24<2||y%16<2{[18,18,18,255]}else{[242,242,242,255]},
            5=>if xx%19<3 || (xx%19<13 && matches!(y%26,3..=5|12..=14|21..=23)){[12,12,12,255]}else{[250,250,250,255]},
            _=> {let t=(xx%w) as f32/w as f32;let v=y as f32/h as f32;
                [(40.0+180.0*t) as u8,(42.0+150.0*(1.0-v)) as u8,(232.0-175.0*t) as u8,255]},
        };bytes.extend_from_slice(&c);
    }} bytes
}
unsafe fn upload(p:&Pipeline,input:&[u8]) {
    p.context.UpdateSubresource(&p.raw.texture,0,None,input.as_ptr().cast(),p.raw.width*4,0);p.prepare();
}
fn linear(v:u8)->f64 {let c=v as f64/255.0;if c<=0.04045{c/12.92}else{((c+0.055)/1.055).powf(2.4)}}
fn luma(p:&[u8])->f64 { 0.2126*linear(p[0])+0.7152*linear(p[1])+0.0722*linear(p[2]) }
fn pix(a:&[u8],w:usize,x:usize,y:usize)->&[u8] {&a[(y*w+x)*4..][..4]}
fn mean(a:&[u8],w:usize,h:usize)->f64 {
    let mut total=0.0;let mut n=0;
    for y in h*2/5..h*3/5 {for x in h..w-h {total+=luma(pix(a,w,x,y));n+=1;}} total/n as f64
}
unsafe fn label_mask(w:u32,h:u32)->AppResult<Vec<u8>> {
    let dc=CreateCompatibleDC(None);ensure(!dc.0.is_null(),"No fixture DC")?;
    let mut info=BITMAPINFO::default();info.bmiHeader=BITMAPINFOHEADER{biSize:size_of::<BITMAPINFOHEADER>() as u32,biWidth:w as i32,biHeight:-(h as i32),biPlanes:1,biBitCount:32,biCompression:BI_RGB.0,..Default::default()};
    let mut bits=std::ptr::null_mut();
    let bmp=CreateDIBSection(Some(dc),&info,DIB_RGB_COLORS,&mut bits,None,0)?;
    let old=SelectObject(dc,HGDIOBJ(bmp.0));
    let font=CreateFontW(-(h as f32*11.0/26.0).round() as i32,0,0,0,500,0,0,0,DEFAULT_CHARSET,OUT_DEFAULT_PRECIS,CLIP_DEFAULT_PRECIS,ANTIALIASED_QUALITY,(DEFAULT_PITCH.0|FF_DONTCARE.0) as u32,w!("Microsoft YaHei UI"));
    let old_font=SelectObject(dc,HGDIOBJ(font.0));std::ptr::write_bytes(bits.cast::<u8>(),0,(w*h*4) as usize);
    SetBkMode(dc,TRANSPARENT);SetTextColor(dc,COLORREF(0xffffff));
    let mut rect=RECT{left:(h as f32*0.92) as i32,top:0,right:w as i32-(h as f32*0.25) as i32,bottom:h as i32};
    let mut text:Vec<u16>="优化识别中".encode_utf16().collect();
    let drawn=DrawTextW(dc,&mut text,&mut rect,DT_CENTER|DT_VCENTER|DT_SINGLELINE);let flushed=GdiFlush().as_bool();
    let mask=std::slice::from_raw_parts(bits as *const u8,(w*h*4) as usize).chunks_exact(4).map(|p|p[0].max(p[1]).max(p[2])).collect::<Vec<_>>();
    SelectObject(dc,old_font);let _=DeleteObject(HGDIOBJ(font.0));SelectObject(dc,old);let _=DeleteObject(HGDIOBJ(bmp.0));let _=DeleteDC(dc);
    ensure(drawn>0&&flushed&&mask.iter().any(|&v|v==255),"Empty fixture label")?;Ok(mask)
}
#[test]
fn actual_liquid_pipeline_has_transmission_lensing_and_input_response()->AppResult<()> {
    unsafe {
        let (device,context)=create_device(None)?;
        let dir=std::env::var_os("GLASS_MATERIAL_FIXTURES").map(|s|std::path::PathBuf::from(s).with_extension("liquid"));
        if let Some(d)=&dir{std::fs::create_dir_all(d)?;}
        let mut metrics=Vec::new();let mut cases=0;
        for (w,h) in [(108u32,26u32),(162,39),(324,78)] {
            let mask=label_mask(w,h)?;
            let p=Pipeline::new(device.clone(),context.clone(),w,h,&mask)?;
            let (rw,rh)=(p.raw.width,p.raw.height);
            for dark in [false,true] {
                upload(&p,&source(rw,rh,0,0));p.render(dark,2.0,false);let black=p.read_rgba(&p.output)?;
                upload(&p,&source(rw,rh,1,0));p.render(dark,3.0,false);let white=p.read_rgba(&p.output)?;
                let gain=mean(&white,w as usize,h as usize)-mean(&black,w as usize,h as usize);
                assert!(gain>if dark{0.35}else{0.70},"Material became paint: dark={dark} transmission={gain}");
                metrics.push(format!("{{\"size\":[{w},{h}],\"dark\":{dark},\"linear_black_white_response\":{gain:.6}}}"));
                for kind in 0..=6 {
                    upload(&p,&source(rw,rh,kind,0));p.render(dark,4.0+kind as f32,false);let bg=p.read_rgba(&p.output)?;
                    p.render(dark,4.0+kind as f32,true);let fg=p.read_rgba(&p.output)?;
                    assert!(fg.chunks_exact(4).all(|v|v[..3].iter().all(|c|*c<=v[3])),"Invalid optical premultiplication");
                    assert_ne!(fg,bg,"Foreground was not rendered");
                    cases+=1;
                    if w==162 {
                        if let Some(d)=&dir {
                            let child=d.join(format!("k{kind}-{}",if dark{"dark"}else{"light"}));std::fs::create_dir_all(&child)?;
                            let legacy=super::super::Pipeline::new(device.clone(),context.clone(),w,h,&mask)?;
                            let input=source(rw,rh,kind,0);
                            legacy.context.UpdateSubresource(&legacy.raw.texture,0,None,input.as_ptr().cast(),rw*4,0);legacy.prepare();legacy.render(dark,4.0+kind as f32,true);legacy.snapshot(&child,1)?;
                            p.snapshot(&child,2)?;
                        }
                    }
                }
            }
            // A true no-lens ablation retains identical blur/tint/lighting/ink.
            let mut flat=Pipeline::new(device.clone(),context.clone(),w,h,&mask)?;
            let src=format!("#define LIQUID_TEST_NO_LENS 1\n{}\n{}",include_str!("glass.hlsl"),include_str!("liquid.hlsl"));
            let b=compile_source(&src,s!("liquid_material_ps"),s!("ps_5_0"))?;let mut shader=None;flat.device.CreatePixelShader(blob_bytes(&b),None,Some(&mut shader))?;flat.base.material=shader.ok_or("No no-lens control")?;
            let input=source(rw,rh,4,0);upload(&p,&input);upload(&flat,&input);
            p.render(false,12.0,false);
            for i in 0..2 {p.context.CopyResource(&flat.adaptation[i].texture,&p.adaptation[i].texture);}
            flat.adapt_index.set(p.adapt_index.get());*flat.motion.borrow_mut()=*p.motion.borrow();
            flat.render(false,12.0,false);
            let curved=p.read_rgba(&p.output)?;let straight=flat.read_rgba(&flat.output)?;
            let different=curved.chunks_exact(4).zip(straight.chunks_exact(4)).filter(|(a,b)|a[3]==255&&b[3]==255&&(0..3).any(|k|a[k].abs_diff(b[k])>8)).count();
            assert!(different>(w*h/30) as usize,"No measurable optical displacement: {different}");
            // Same timestamp is a pure re-render. Input history cannot drift inside SAVE.
            p.render(false,12.0,false);assert_eq!(curved,p.read_rgba(&p.output)?);
            if w==162 {if let Some(d)=&dir{
                let child=d.join("lensing-ablation");std::fs::create_dir_all(&child)?;flat.snapshot(&child,1)?;p.snapshot(&child,2)?;
            }}
            // Pointer press and release act on material even with the voice bars OFF.
            let still=curved;
            for i in 1..=40 {p.render_input(false,12.0+i as f32/120.0,false,[0.8,0.6],true,[0.65,0.1],false);}
            let pressed=p.read_rgba(&p.output)?;assert_ne!(still,pressed,"Interaction only changed the foreground, not liquid material");
            for i in 41..=480{p.render_input(false,12.0+i as f32/120.0,false,[0.3,0.18],false,[0.0;2],false);}
            let settled=p.read_rgba(&p.output)?;
            let max=still.iter().zip(&settled).map(|(a,b)|a.abs_diff(*b)).max().unwrap_or(0);
            assert!(max<=1,"Material failed to settle without perpetual idle oscillation: {max}");
            metrics.push(format!("{{\"size\":[{w},{h}],\"lensing_changed_pixels\":{different},\"settled_max_error\":{max}}}"));
        }
        // Actual shader frames, not a generated illustration. Synthetic input
        // playback on WARP, not a capture/present performance claim.
        if let Some(d)=&dir {
            let w=270;let h=60;let mask=label_mask(w,h)?;
            let p=Pipeline::new(device.clone(),context.clone(),w,h,&mask)?;
            let frames=d.join("input-motion");std::fs::create_dir_all(&frames)?;
            for i in 0..72 {
                let t=i as f32/30.0;
                let pressed=(12..40).contains(&i);let pt=if pressed{[0.78,0.55]}else{[0.30,0.18]};
                upload(&p,&source(p.raw.width,p.raw.height,6,(i*3) as u32));
                p.render_input(false,t+1.0,true,pt,pressed,if pressed{[0.6,0.1]}else{[0.0;2]},false);p.snapshot(&frames,i+1)?;
            }
        }
        let json=format!("{{\"material\":\"LENS_TRANSMISSION_1\",\"scope\":\"actual runtime shader on WARP, synthetic backgrounds and injected input; not Apple parity, real desktop or FPS acceptance\",\"cases\":{cases},\"metrics\":[{}]}}",metrics.join(","));
        eprintln!("[liquid-optics-contract] {json}");if let Some(d)=&dir{std::fs::write(d.join("metrics.json"),json)?;}
    } Ok(())
}
