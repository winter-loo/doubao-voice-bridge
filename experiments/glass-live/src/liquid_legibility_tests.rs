// Included in optical::tests; reuse its real runtime shader and GDI label.
#[test]
fn optical_glyph_contrast_is_local_and_scene_transmission_remains()->AppResult<()> {
    unsafe {
        let (device,context)=create_device(None)?;
        let mut minimum=100.0f64;let mut checked=0usize;
        let encode=|v:f64|->u8{((if v<=0.0031308{v*12.92}else{1.055*v.powf(1.0/2.4)-0.055})*255.0).round() as u8};
        let inks=[[encode(0.010),encode(0.016),encode(0.023)],[encode(0.95),encode(0.97),encode(1.0)]];
        for (w,h) in [(108u32,26u32),(162,39),(324,78)] {
            let mask=label_mask(w,h)?;
            let p=Pipeline::new(device.clone(),context.clone(),w,h,&mask)?;
            let mut under=Pipeline::new(device.clone(),context.clone(),w,h,&mask)?;
            let src=format!("#define LIQUID_TEST_INK_OFF 1\n{}\n{}",include_str!("glass.hlsl"),include_str!("liquid.hlsl"));
            let code=compile_source(&src,s!("liquid_material_ps"),s!("ps_5_0"))?;let mut shader=None;
            under.device.CreatePixelShader(blob_bytes(&code),None,Some(&mut shader))?;
            under.base.material=shader.ok_or("Missing material-only legibility control")?;
            for kind in 0..=6 {for dark in [false,true] {
                let input=source(p.raw.width,p.raw.height,kind,0);upload(&p,&input);upload(&under,&input);
                let time=3.0+kind as f32;
                p.render(dark,time,true);
                for i in 0..2{p.context.CopyResource(&under.adaptation[i].texture,&p.adaptation[i].texture);}
                under.adapt_index.set(p.adapt_index.get());*under.motion.borrow_mut()=*p.motion.borrow();
                under.render(dark,time,true);
                let fg=p.read_rgba(&p.output)?;let bg=under.read_rgba(&under.output)?;let mut count=0;
                for y in h/3..h*2/3 {for x in h..w-h/3 {
                    let a=pix(&fg,w as usize,x as usize,y as usize);let b=pix(&bg,w as usize,x as usize,y as usize);
                    if a[3]==255 && inks.iter().any(|ink|(0..3).all(|k|a[k].abs_diff(ink[k])<=1)) {
                        let ya=luma(a);let yb=luma(b);let ratio=(ya.max(yb)+0.05)/(ya.min(yb)+0.05);
                        assert!(ratio>=4.5,"Opaque foreground contrast {ratio}, size={w}x{h}, kind={kind}, dark={dark}, xy={x},{y}");
                        minimum=minimum.min(ratio);count+=1;
                    }
                }}
                assert!(count>10,"No opaque foreground samples: {w}x{h} kind={kind}");checked+=count;
            }}
        }
        let json=format!("{{\"scope\":\"opaque ink samples versus same-state ink-off material; not full accessibility certification\",\"min_contrast\":{minimum:.6},\"checked_pixels\":{checked}}}");
        eprintln!("[liquid-legibility] {json}");
        if let Some(dir)=std::env::var_os("GLASS_MATERIAL_FIXTURES").map(|s|std::path::PathBuf::from(s).with_extension("liquid")){
            std::fs::create_dir_all(&dir)?;std::fs::write(dir.join("legibility.json"),json)?;
        }
    }Ok(())
}
#[test]
fn glyph_support_does_not_create_a_capsule_backplate() {
    let (w,h)=(162,39);let mut mask=vec![0;w*h];mask[19*w+81]=255;
    let s=make_support(&mask,w,h);
    assert_eq!(s[19*w+81],255);
    assert!(s.iter().filter(|v|**v>0).count()<200);
    assert!(make_support(&vec![0;w*h],w,h).iter().all(|v|*v==0));
}
