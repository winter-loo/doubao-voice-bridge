//! Current runtime contracts, separate from the frozen V1/V2 historical tests.
//! WARP, generated pixels, fixed phase. No desktop capture, OCR or FPS claims.
use super::*;
use material_config::Profile;

fn rgba(pixels: &[u8], w: u32, x: u32, y: u32) -> &[u8] {
    &pixels[((y*w+x)*4) as usize..][..4]
}
fn luminance(p: &[u8]) -> f64 {
    fn linear(v: u8) -> f64 {
        let v=v as f64/255.0;
        if v<=0.04045 {v/12.92} else {((v+0.055)/1.055).powf(2.4)}
    }
    0.2126*linear(p[0])+0.7152*linear(p[1])+0.0722*linear(p[2])
}
fn inset(w: u32,h: u32,x: u32,y: u32) -> f32 {
    let qx=x as f32+0.5-w as f32*0.5;
    let qy=y as f32+0.5-h as f32*0.5;
    let half=(w-h) as f32*0.5;
    h as f32*0.5-((qx-qx.clamp(-half,half)).powi(2)+qy*qy).sqrt()
}
fn smooth(a: f32,b: f32,v: f32) -> f32 {
    let t=((v-a)/(b-a)).clamp(0.0,1.0); t*t*(3.0-2.0*t)
}
fn expected_mask(w: u32,h: u32,x: u32,y: u32) -> f32 {
    let c=material_config::CONTENT;
    let half=[c.extent[0]*w as f32*0.5,c.extent[1]*h as f32*0.5];
    let r=(c.corner*h as f32).min(half[0]).min(half[1]);
    let q=[(x as f32+0.5-c.center[0]*w as f32).abs()-half[0]+r,
           (y as f32+0.5-c.center[1]*h as f32).abs()-half[1]+r];
    let d=(q[0].max(0.0).powi(2)+q[1].max(0.0).powi(2)).sqrt()+q[0].max(q[1]).min(0.0)-r;
    (1.0-smooth(-c.feather*h as f32,0.0,d))*smooth(0.08*h as f32,0.20*h as f32,inset(w,h,x,y).max(0.0))
}
fn content_mask(w: u32,h: u32) -> Vec<u8> {
    // Synthetic text with opaque and antialiased strokes. Also exercise original
    // procedural waveform through foreground=true; no external font dependency.
    (0..w*h).map(|i| {
        let (x,y)=(i%w,i/w);
        if y>h/3 && y<h*2/3 && x>h && x<w-h/2 {
            match (x*162/w)%7 {0|1=>255,2=>128,_=>0}
        } else {0}
    }).collect()
}
unsafe fn frame(p: &Pipeline,dark: bool,foreground: bool) -> AppResult<Vec<u8>> {
    p.render(dark,1.25,foreground); p.read_rgba(&p.output)
}
fn check_shape(before: &[u8],after: &[u8]) {
    for (a,b) in before.chunks_exact(4).zip(after.chunks_exact(4)) {
        assert_eq!(a[3],b[3],"Candidate changed silhouette/alpha");
        assert!(b[..3].iter().all(|v|*v<=b[3]),"Invalid premultiplication");
        if b[3]==0 { assert_eq!(&b[..3],&[0,0,0]); }
    }
}
fn response(a: &[u8],b: &[u8],w: u32,h: u32,rim: bool) -> f64 {
    let mut sum=0.0; let mut n=0;
    for y in 0..h {for x in 0..w {
        let use_pixel=if rim {let d=inset(w,h,x,y); d>=0.8 && d<=h as f32*0.08}
            else {y>=h/3 && y<h*2/3 && x>=h && x<w-h};
        if use_pixel {sum+=(luminance(rgba(a,w,x,y))-luminance(rgba(b,w,x,y))).abs(); n+=1;}
    }}
    assert!(n>20,"Empty response region"); sum/n as f64
}

#[test]
fn gpu_content_mask_matches_soft_geometry() -> AppResult<()> {
    unsafe {
        let (device,context)=create_device(None)?;
        for (w,h) in [(108,26),(162,39),(324,78)] {
            let mut p=Pipeline::new(device.clone(),context.clone(),w,h,&vec![0;(w*h) as usize])?;
            let source=format!("{}\n{}\n{}",material_config::hlsl_header()?,
                include_str!("glass.hlsl"),include_str!("configured_material.hlsl"));
            let code=compile_source(&source,s!("configured_content_mask_ps"),s!("ps_5_0"))?;
            let mut shader=None; p.device.CreatePixelShader(blob_bytes(&code),None,Some(&mut shader))?;
            p.material=shader.ok_or("No mask-test shader")?;
            upload(&p,&input(p.raw.width,p.raw.height,2,0),false);
            let pixels=frame(&p,false,false)?;
            let mut partial=0; let mut core=0;
            for y in 0..h {for x in 0..w {
                let v=rgba(&pixels,w,x,y)[0];
                let expected=(expected_mask(w,h,x,y)*255.0).round() as u8;
                assert!(v.abs_diff(expected)<=1,"Content mask disagrees with physical geometry at {w}x{h} ({x},{y})");
                if inset(w,h,x,y)<=0.08*h as f32 {assert_eq!(v,0,"Veil reached thin rim");}
                if v>0 && v<255 {partial+=1;} if v==255 {core+=1;}
            }}
            assert!(partial>(h*2) && core>w,"Hard/empty mask instead of a soft band");
        }
    }
    Ok(())
}

#[test]
fn v23_current_runtime_and_stage_contracts() -> AppResult<()> {
    unsafe {
        let (device,context)=create_device(None)?;
        let dir=std::env::var_os("GLASS_MATERIAL_FIXTURES")
            .map(|p|std::path::PathBuf::from(p).with_extension("issue11-v23"));
        if let Some(d)=&dir {std::fs::create_dir_all(d)?;}
        let mut records=Vec::new(); let mut cases=0;
        let mut min_contrast=f64::INFINITY;
        let mut max_veil_light=0u8; let mut max_veil_dark=0u8;
        for (w,h) in [(108,26),(162,39),(324,78)] {
            let mask=content_mask(w,h);
            let old=reference_pipeline(device.clone(),context.clone(),w,h,&mask)?;
            let veil=configured_pipeline(device.clone(),context.clone(),w,h,&mask,Profile::Veil)?;
            let rim=configured_pipeline(device.clone(),context.clone(),w,h,&mask,Profile::Rim)?;
            // Deliberately use the SAME constructor/entry/defaults as live mode.
            let current=Pipeline::new(device.clone(),context.clone(),w,h,&mask)?;
            assert_eq!(old.padding,current.padding,"Capture ROI expanded");
            assert_eq!((current.raw.width,current.raw.height),(old.raw.width,old.raw.height));
            for kind in 0..=5 {
                let input=input(current.raw.width,current.raw.height,kind,0);
                for p in [&old,&veil,&rim,&current] {upload(p,&input,false);}
                for dark in [false,true] {
                    let a=frame(&old,dark,false)?; let b=frame(&veil,dark,false)?;
                    let r=frame(&rim,dark,false)?; let c=frame(&current,dark,false)?;
                    check_shape(&a,&b); check_shape(&a,&r); check_shape(&a,&c);
                    for y in 0..h {for x in 0..w {
                        let pa=rgba(&a,w,x,y); let pb=rgba(&b,w,x,y);
                        let pr=rgba(&r,w,x,y); let pc=rgba(&c,w,x,y);
                        let delta=(0..3).map(|k|pa[k].abs_diff(pb[k])).max().unwrap();
                        if dark {max_veil_dark=max_veil_dark.max(delta);} else {max_veil_light=max_veil_light.max(delta);}
                        assert!(delta<=if dark {12} else {4},"Veil lift exceeded stage-2 bound: {delta}");
                        if expected_mask(w,h,x,y)==0.0 {
                            assert!(pa.iter().zip(pb).all(|(a,b)|a.abs_diff(*b)<=1),"Veil altered area outside its support");
                        }
                        if inset(w,h,x,y)>=0.20*h as f32 {
                            assert!(pb.iter().zip(pr).all(|(a,b)|a.abs_diff(*b)<=1),"Rim stage changed protected body");
                            assert_eq!(pc[3],255,"Raw backdrop leaked through center alpha");
                        }
                        // On uniform backgrounds bound the spatial derivative of
                        // the added veil, rather than confusing rim/text with it.
                        if kind<=2 && x+1<w && inset(w,h,x,y)>0.08*h as f32 {
                            for k in 0..3 {
                                let d0=pb[k] as i32-pa[k] as i32;
                                let d1=rgba(&b,w,x+1,y)[k] as i32-rgba(&a,w,x+1,y)[k] as i32;
                                assert!((d1-d0).abs()<=5,"Horizontal hard veil patch");
                            }
                        }
                        if kind<=2 && y+1<h && inset(w,h,x,y)>0.08*h as f32 {
                            for k in 0..3 {
                                let d0=pb[k] as i32-pa[k] as i32;
                                let d1=rgba(&b,w,x,y+1)[k] as i32-rgba(&a,w,x,y+1)[k] as i32;
                                assert!((d1-d0).abs()<=5,"Vertical hard veil patch");
                            }
                        }
                    }}
                    let old_fg=frame(&old,dark,true)?; let fg=frame(&current,dark,true)?;
                    for (i,m) in mask.iter().enumerate() {if *m==255 {
                        assert_eq!(&old_fg[i*4..i*4+4],&fg[i*4..i*4+4],"Opaque foreground changed");
                        let lf=luminance(&fg[i*4..i*4+4]); let lb=luminance(&c[i*4..i*4+4]);
                        let contrast=(lf.max(lb)+0.05)/(lf.min(lb)+0.05);
                        min_contrast=min_contrast.min(contrast);
                        assert!(contrast>=7.0,"Synthetic foreground contrast below design bound: {contrast}");
                    }}
                    if kind==4 {
                        let left=rgba(&c,w,h*3/4,h/2); let right=rgba(&c,w,w-h*3/4,h/2);
                        assert!(left[0] as i32-left[2] as i32>5 && right[2] as i32-right[0] as i32>5,"Low-frequency color response lost");
                    }
                    if kind==2 {
                        let center=rgba(&c,w,w/2,h/2);
                        if dark {assert!(center[..3].iter().all(|v|*v<110));}
                        else {assert!(center[..3].iter().all(|v|*v>=230));}
                    }
                    // A repeat with identical pixels and phase must be identical.
                    assert_eq!(fg,frame(&current,dark,true)?,"Material has hidden temporal state");
                    if w==162 {
                        if let Some(d)=&dir {
                            let name=["black","gray","white","stripes","colors","text"][kind as usize];
                            let d=d.join(format!("{name}-{}",if dark {"dark"} else {"light"}));
                            std::fs::create_dir_all(&d)?;
                            for (number,p) in [(1,&old),(2,&veil),(3,&rim),(4,&current)] {
                                p.render(dark,1.25,true); p.snapshot(&d,number)?;
                            }
                        }
                    }
                    cases+=1;
                }
            }
            // Black/white changes distinguish real transmission from a blank
            // constant panel. This also measures the veil's contrast contraction.
            for dark in [false,true] {
                let mut old_frames=Vec::new(); let mut veil_frames=Vec::new();
                for kind in [0,2] {
                    let input=input(current.raw.width,current.raw.height,kind,0);
                    upload(&old,&input,false); upload(&veil,&input,false);
                    old_frames.push(frame(&old,dark,false)?); veil_frames.push(frame(&veil,dark,false)?);
                }
                let before=response(&old_frames[0],&old_frames[1],w,h,false);
                let after=response(&veil_frames[0],&veil_frames[1],w,h,false);
                assert!(before>0.025 && after>before*0.70,"Transmission disappeared");
                assert!(after<=before*0.99+0.0005,"Veil failed to contract scene-dependent contrast");
                records.push(format!("{{\"w\":{w},\"h\":{h},\"dark\":{dark},\"center_response_v22\":{before:.6},\"center_response_veil\":{after:.6}}}"));
                let mut prior=Vec::new(); let mut now=Vec::new();
                for invert in [false,true] {
                    let mut input=Vec::new();
                    for _y in 0..current.raw.height {for x in 0..current.raw.width {
                        let v=if (x/8%2==0)^invert {0} else {255}; input.extend_from_slice(&[v,v,v,255]);
                    }}
                    upload(&rim,&input,false); upload(&current,&input,false);
                    prior.push(frame(&rim,dark,false)?); now.push(frame(&current,dark,false)?);
                }
                let before=response(&prior[0],&prior[1],w,h,true);
                let after=response(&now[0],&now[1],w,h,true);
                assert!(after<=before+0.001,"Candidate amplified edge detail");
                let center_before=response(&prior[0],&prior[1],w,h,false);
                let center_after=response(&now[0],&now[1],w,h,false);
                assert!(center_after<=center_before+0.0005,"Candidate sharpened protected center");
                records.push(format!("{{\"w\":{w},\"h\":{h},\"dark\":{dark},\"rim_response_before\":{before:.6},\"rim_response_current\":{after:.6},\"center_detail_before\":{center_before:.6},\"center_detail_current\":{center_after:.6}}}"));
            }
            // A -> B -> A generated movement is NOT a display smoothness test.
            // It proves no feedback/history was introduced by material changes.
            let a=input(current.raw.width,current.raw.height,5,0);
            let b=input(current.raw.width,current.raw.height,5,7);
            upload(&current,&a,false); let a1=frame(&current,false,true)?;
            upload(&current,&b,false); let _=frame(&current,false,true)?;
            upload(&current,&a,false); assert_eq!(a1,frame(&current,false,true)?);
        }
        let summary=format!("{{\"stage\":\"2-4-candidate\",\"scope\":\"offscreen WARP generated fixtures; not real Notepad, DWM, motion or user acceptance\",\"cases\":{cases},\"max_veil_light_rgb_shift\":{max_veil_light},\"max_veil_dark_rgb_shift\":{max_veil_dark},\"min_synthetic_foreground_contrast\":{min_contrast:.6},\"alpha_exact\":true,\"opaque_foreground_exact\":true,\"metrics\":[{}]}}",records.join(","));
        eprintln!("[issue11-current] {summary}");
        if let Some(d)=&dir {
            std::fs::write(d.join("metrics.json"),summary)?;
            std::fs::write(d.join("parameters.txt"),material_config::hlsl_header()?)?;
        }
    }
    Ok(())
}
