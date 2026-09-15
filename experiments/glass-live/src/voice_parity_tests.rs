use super::*;
#[test]
fn production_optimizing_preserves_approved_optical_pixels()->AppResult<()>{unsafe{
    let (d,c)=create_device(None)?;let (w,h)=(162u32,39u32);
    let mask=crate::voice_window::content_mask(w,h,1.5,crate::voice_model::Phase::Optimizing)?;
    let current=Pipeline::new_voice(d.clone(),c.clone(),w,h,&mask)?;
    let mut approved=Pipeline::new(d,c,w,h,&mask)?;
    let source=format!("{}\n{}",include_str!("glass.hlsl"),include_str!("liquid_reference.hlsl"));
    let b=compile_source(&source,s!("liquid_material_ps"),s!("ps_5_0"))?;let mut shader=None;
    approved.device.CreatePixelShader(blob_bytes(&b),None,Some(&mut shader))?;
    approved.base.material=shader.ok_or("Missing approved reference shader")?;
    let (rw,rh)=(current.raw.width,current.raw.height);let mut max=0u8;
    for dark in [false,true]{for kind in 0..4{
        let mut input:Vec<u8>=vec![0u8;(rw*rh*4) as usize];
        for y in 0..rh{for x in 0..rw{
            let bgra:[u8;4]=match kind{
                0=>[250,250,250,255],1=>[4,4,4,255],
                2=>if x<rw/2{[64,48,220,255]}else{[224,112,32,255]},
                _=>if x%24<2||y%16<2{[12,12,12,255]}else{[245,245,245,255]},
            };input[((y*rw+x)*4) as usize..][..4].copy_from_slice(&bgra);
        }}
        // Both paths must receive the intended byte-addressed texture, not two
        // identically corrupted inputs that could still give a false parity pass.
        for p in [&current,&approved]{crate::voice_render_checks::upload_fixture_checked(p,&input)?;}
        for frame in 0..4{
            let t=2.+kind as f32+frame as f32*0.04+if dark{10.}else{0.};
            current.render_voice(dark,t,crate::voice_model::Phase::Optimizing,0.,1.);approved.render(dark,t,true);
            let a=current.read_rgba(&current.output)?;let b=approved.read_rgba(&approved.output)?;
            max=max.max(a.iter().zip(&b).map(|(a,b)|a.abs_diff(*b)).max().unwrap_or(0));
        }
    }}
    assert!(max<=1,"Production integration changed approved material pixels: max={max}");
    eprintln!("[voice-parity] approved 7a3d03c shader vs production optimizing; 32 cases; byte-validated backgrounds; max RGBA error={max}; synthetic WARP");
}Ok(())}
