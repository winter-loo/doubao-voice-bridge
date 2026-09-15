//! Executed production shader checks. All inputs are byte-checked generated data.
//! This is not a recording of the user's display and contains no audio/network.
use super::*;
use crate::voice_render_checks::upload_fixture_checked;

fn fixture(w:u32,h:u32,kind:u32)->Vec<u8> {
    let mut out=vec![0u8;(w*h*4) as usize];
    for y in 0..h {for x in 0..w {
        let c:[u8;4]=match kind {
            0=>[255,255,255,255],1=>[0,0,0,255],2=>[86,183,9,255],
            3=>if x%19<3||(x%19<13&&matches!(y%26,3..=5|12..=14|21..=23)){[12,12,12,255]}else{[250,250,250,255]},
            4=>if x<w/2{[64,48,220,255]}else{[224,112,32,255]},
            _=>[232,232,232,255],
        };out[((y*w+x)*4) as usize..][..4].copy_from_slice(&c);
    }}out
}
fn original(index:usize)->[u8;3] {
    let a=[67.0f32,222.0,210.0];let b=[100.0f32,141.0,255.0];
    std::array::from_fn(|k|(a[k]+(b[k]-a[k])*index as f32/19.).round() as u8)
}
fn pixel(a:&[u8],w:u32,x:u32,y:u32)->&[u8] {&a[((y*w+x)*4) as usize..][..4]}
fn linear(v:u8)->f64 {let f=v as f64/255.;if f<=0.04045{f/12.92}else{((f+0.055)/1.055).powf(2.4)}}
fn luma(a:&[u8])->f64 {linear(a[0])*0.2126+linear(a[1])*0.7152+linear(a[2])*0.0722}
unsafe fn shader(p:&AdaptivePipeline,define:&str)->AppResult<ID3D11PixelShader> {
    let text=format!("#define LIQUID_VOICE_CONTENT 1\n{define}\n{}\n{}\n{}\n{}",include_str!("glass.hlsl"),include_str!("voice_content.hlsl"),include_str!("liquid.hlsl"),include_str!("adaptive.hlsl"));
    let b=compile_source(&text,s!("adaptive_material_ps"),s!("ps_5_0"))?;let mut shader=None;
    p.device.CreatePixelShader(blob_bytes(&b),None,Some(&mut shader))?;
    Ok(shader.ok_or("No adaptive test control")?)
}
unsafe fn advance(p:&AdaptivePipeline,dark:bool,from:f32,to:f32,phase:Phase) {
    let mut t=from;
    while t+1./120.<to {t+=1./120.;p.render_voice(dark,t,phase,0.,1.);}
    p.render_voice(dark,to,phase,0.,1.);
}
fn max_diff(a:&[u8],b:&[u8])->u8 {a.iter().zip(b).map(|(a,b)|a.abs_diff(*b)).max().unwrap_or(0)}

pub(super) unsafe fn verify(directory:Option<&Path>)->AppResult<()> {
    let (device,context)=create_device(None)?;
    if let Some(d)=directory {std::fs::create_dir_all(d)?;}
    let mut frame_count=0usize;let mut palette_cases=0usize;let mut palette_pixels=0usize;let mut palette_error=0u8;
    let mut shadow_count=0usize;let mut min_contrast=100f64;let mut ink_pixels=0usize;
    let mut records=Vec::new();
    let times=[0.,0.05,0.1,0.133,0.2,0.3,0.4,0.6,0.8];
    for (w,h) in [(108u32,26u32),(162,39),(324,78)] {for dark in [false,true] {for kind in 0..6 {
        let mask=crate::voice_window::content_mask(w,h,h as f32/26.,Phase::Optimizing)?;
        let mut p=AdaptivePipeline::new_voice(device.clone(),context.clone(),w,h,&mask)?;
        let control=Pipeline::new_voice(device.clone(),context.clone(),w,h,&mask)?;
        let input=fixture(p.raw.width,p.raw.height,kind);
        upload_fixture_checked(&p,&input)?;upload_fixture_checked(&control,&input)?;
        let mut last=0f32;
        let child=directory.filter(|_|w==162).map(|d|d.join(format!("k{kind}-{}",if dark{"dark"}else{"light"})));
        if let Some(d)=&child {std::fs::create_dir_all(d)?;}
        for (n,&time) in times.iter().enumerate() {
            advance(&p,dark,last,time,Phase::Optimizing);
            // Baseline uses the same submitted timestamps and byte-verified input.
            let mut t=last;while t+1./120.<time {t+=1./120.;control.render_voice(dark,t,Phase::Optimizing,0.,1.);}
            control.render_voice(dark,time,Phase::Optimizing,0.,1.);last=time;
            let a=p.read_rgba(&p.canvas)?;
            ensure(a.chunks_exact(4).all(|v|v[..3].iter().all(|c|*c<=v[3])),"Adaptive canvas is not premultiplied")?;
            let cw=p.canvas.width;let ch=p.canvas.height;
            for x in 0..cw {ensure(pixel(&a,cw,x,0)[3]==0&&pixel(&a,cw,x,ch-1)[3]==0,"Exterior shadow is clipped at canvas edge")?;}
            for y in 0..ch {ensure(pixel(&a,cw,0,y)[3]==0&&pixel(&a,cw,cw-1,y)[3]==0,"Exterior shadow is clipped at canvas side")?;}
            let core=p.read_rgba(&p.output)?;
            let probe=pixel(&core,w,w-h/5,h/2);
            records.push(format!("{{\"size\":[{w},{h}],\"dark\":{dark},\"background\":{kind},\"time_ms\":{:.3},\"body_probe_rgba\":{:?}}}",time*1000.,probe));
            if let Some(d)=&child {
                let composite=p.composite_fixture()?;
                crate::png::write(&d.join(format!("adaptive-{n:02}.png")),p.raw.width,p.raw.height,&composite)?;
                control.snapshot(d,n as u32+1)?;
            }
            frame_count+=1;
        }
        // Freeze content for stability checks; an animating optimizing icon is not
        // evidence that an otherwise settled material keeps breathing.
        p.set_voice_mask(&vec![0u8;(w*h) as usize])?;
        advance(&p,dark,0.8,1.6,Phase::Activating);let stable=p.read_rgba(&p.canvas)?;
        advance(&p,dark,1.6,2.4,Phase::Activating);let held=p.read_rgba(&p.canvas)?;
        ensure(max_diff(&stable,&held)<=1,"Stable background causes adaptive material drift")?;
        let m=p.layout.margin;let cw=p.canvas.width;
        if kind==0 {
            let mut count=0;
            for y in m+h..p.canvas.height {for x in m..m+w {
                let a=pixel(&held,cw,x,y)[3];if a>=3 {count+=1;}
            }}
            ensure(count>(w/2) as usize,"White-background exterior shadow has no real output area")?;shadow_count+=count;
            let center=p.read_rgba(&p.output)?;
            if !dark {ensure(pixel(&center,w,w/2,h/2)[..3].iter().all(|v|*v>=245),"White material stabilized as an opaque gray plate")?;}
        }
        if kind==2 {
            let core=p.read_rgba(&p.output)?;let c=pixel(&core,w,w/2,h/2);
            ensure(c[1]>c[0]+60 && c[1]>c[2]+25,"Adaptive material washed out the green source")?;
        }
        // Same timestamp = same material state, regardless of theme export order.
        p.render_voice(dark,2.4,Phase::Activating,0.,1.);let same=p.read_rgba(&p.canvas)?;
        ensure(held==same,"Repeated-frame render advanced adaptive history")?;
        p.render_voice(!dark,2.4,Phase::Activating,0.,1.);
        p.render_voice(dark,2.4,Phase::Activating,0.,1.);
        ensure(held==p.read_rgba(&p.canvas)?,"Theme pair rendering mutated shared frame history")?;
        for time in [2.5f32,2.9] {for level in [0f32,1.] {
            p.render_voice(dark,time,Phase::Listening,level,1.);let core=p.read_rgba(&p.output)?;
            let scale=h as f32/26.;
            for i in 0..20 {
                let x=((w as f32-78.*scale)*0.5+(i as f32*4.+1.)*scale).floor() as u32;
                let a=pixel(&core,w,x,h/2);let b=original(i);
                ensure(a[3]==255,"Recording stroke core lost full coverage")?;
                for k in 0..3 {let e=a[k].abs_diff(b[k]);palette_error=palette_error.max(e);ensure(e<=1,"Original colored waveform changed in adaptive renderer")?;}
                palette_pixels+=1;
            }palette_cases+=1;
        }}
        // Opaque text contrast is checked against the SAME state with ink disabled.
        p.set_voice_mask(&mask)?;advance(&p,dark,2.9,3.2,Phase::Optimizing);
        let fg=p.read_rgba(&p.output)?;let real=p.shader.clone();p.shader=shader(&p,"#define LIQUID_TEST_INK_OFF 1")?;
        p.render_voice(dark,3.2,Phase::Optimizing,0.,1.);let bg=p.read_rgba(&p.output)?;p.shader=real;
        let encode=|x:f32|->u8{((if x<=0.0031308{12.92*x}else{1.055*x.powf(1./2.4)-0.055})*255.).round() as u8};
        let inks=[[encode(0.010),encode(0.016),encode(0.023)],[encode(0.95),encode(0.97),255]];
        let mut checked=0;
        for y in h/3..h*2/3 {for x in h..w-h/3 {
            let a=pixel(&fg,w,x,y);let b=pixel(&bg,w,x,y);
            if a[3]==255&&inks.iter().any(|i|(0..3).all(|k|a[k].abs_diff(i[k])<=1)) {
                let ya=luma(a);let yb=luma(b);let ratio=(ya.max(yb)+0.05)/(ya.min(yb)+0.05);
                ensure(ratio>=4.5,"Adaptive opaque text lost contrast against its local material")?;
                min_contrast=min_contrast.min(ratio);checked+=1;
            }
        }}ensure(checked>10,"No opaque adaptive text coverage was tested")?;ink_pixels+=checked;
        p.render_voice(dark,3.5,Phase::Hidden,0.,0.);
        ensure(p.read_rgba(&p.canvas)?.iter().all(|x|*x==0),"Hidden adaptive canvas retains shadow or content")?;
    }}}
    // Stationary material-only step/reversal test. This is additional engineering
    // evidence, not a claim that the iPhone clips contained these scene switches.
    let p=AdaptivePipeline::new_voice(device.clone(),context.clone(),162,39,&vec![0u8;162*39])?;
    let mut elapsed=0f32;let mut step_records=Vec::new();
    for (sequence,&kind) in [0,1,0,2,0,1,2,0].iter().enumerate() {
        upload_fixture_checked(&p,&fixture(p.raw.width,p.raw.height,kind))?;
        for frame in 0..=36 {
            elapsed+=1./60.;p.render_voice(false,elapsed,Phase::Activating,0.,1.);
            let pixels=p.read_rgba(&p.output)?;let core=pixel(&pixels,162,81,19);
            if kind==2 {ensure(core[1]>core[0]+50,"Scene pixels trail the previous white/black background")?;}
            step_records.push(format!("{{\"sequence\":{sequence},\"kind\":{kind},\"frame\":{frame},\"t\":{elapsed:.6},\"rgba\":{core:?}}}"));
            if let Some(d)=directory {
                let path=d.join("steps");std::fs::create_dir_all(&path)?;
                crate::png::write(&path.join(format!("step-{sequence}-{frame:02}.png")),p.raw.width,p.raw.height,&p.composite_fixture()?)?;
            }
        }
    }
    // Reduced motion gets the stable state on frame zero; no forced opacity pulse.
    let p=AdaptivePipeline::new_voice(device,context,162,39,&vec![0u8;162*39])?;
    p.inner.reduced_motion.set(true);upload_fixture_checked(&p,&fixture(p.raw.width,p.raw.height,0))?;
    p.render_voice(false,0.,Phase::Activating,0.,1.);let a=p.read_rgba(&p.canvas)?;
    p.render_voice(false,0.6,Phase::Activating,0.,1.);
    ensure(a==p.read_rgba(&p.canvas)?,"Reduced motion still animates material entrance")?;
    let summary=format!("{{\"scope\":\"actual production HLSL on WARP; generated byte-validated source; not live capture or hardware FPS\",\"material\":\"LENS_ADAPTIVE_2\",\"entrance_cases\":{frame_count},\"palette_cases\":{palette_cases},\"palette_core_pixels\":{palette_pixels},\"max_palette_byte_error\":{palette_error},\"exterior_shadow_pixels_checked\":{shadow_count},\"opaque_ink_pixels\":{ink_pixels},\"min_opaque_ink_contrast\":{min_contrast:.6},\"frames\":[{}],\"steps\":[{}]}}",records.join(","),step_records.join(","));
    if let Some(d)=directory {std::fs::write(d.join("metrics.json"),&summary)?;}
    eprintln!("[adaptive-glass] PASS; {frame_count} entrance cases; {palette_cases} waveform cases / {palette_pixels} cores; palette error={palette_error}; exterior shadow={shadow_count}; text contrast minimum={min_contrast:.4}; held-source stability, same-frame pairing, step/reversal, reduced-motion and hidden alpha checked");
    Ok(())
}
#[cfg(test)] mod tests {
    #[test] fn actual_production_adaptive_contracts(){unsafe{
        let dir=std::env::var_os("GLASS_MATERIAL_FIXTURES").map(|p|std::path::PathBuf::from(p).with_extension("adaptive"));
        super::verify(dir.as_deref()).unwrap();
    }}
}
