//! Additional checks against the CURRENT production AdaptivePipeline, not the
//! historical optical control. Generated fixtures and states only; never capture.
use super::*;
use crate::voice_render_checks::upload_fixture_checked;

fn input(w:u32,h:u32,kind:u32)->Vec<u8> {
    let mut bytes=vec![0u8;(w*h*4) as usize];
    for y in 0..h {for x in 0..w {
        let color:[u8;4]=match kind {
            0=>[255,255,255,255],1=>[0,0,0,255],2=>[86,183,9,255],
            _=>if x%13<2||y%11<2{[20,20,20,255]}else{[235,235,235,255]},
        };
        bytes[((y*w+x)*4) as usize..][..4].copy_from_slice(&color);
    }}bytes
}
fn delta(a:&[u8],b:&[u8])->usize {
    assert_eq!(a.len(),b.len());a.chunks_exact(4).zip(b.chunks_exact(4)).filter(|(a,b)|a!=b).count()
}
unsafe fn frames(p:&AdaptivePipeline,dark:bool,from:f32,to:f32,phase:Phase) {
    let count=((to-from).max(0.)*120.).ceil().max(1.) as usize;
    for i in 1..=count {p.render_voice(dark,from+(to-from)*i as f32/count as f32,phase,0.,1.);}
}

pub(super) unsafe fn verify(directory:Option<&Path>)->AppResult<()> {
    let (device,context)=create_device(None)?;
    let mut states=0;let mut changed_by_lens=0;
    for dark in [false,true] {for kind in 0..4 {
        let mut p=AdaptivePipeline::new_voice(device.clone(),context.clone(),162,39,&vec![0u8;162*39])?;
        upload_fixture_checked(&p,&input(p.raw.width,p.raw.height,kind))?;
        let mut time=0f32;
        for phase in [Phase::Activating,Phase::Listening,Phase::Optimizing,Phase::Completed,Phase::Failed] {
            let mask=crate::voice_window::content_mask(162,39,1.5,phase)?;
            p.set_voice_mask(&mask)?;
            frames(&p,dark,time,time+0.35,phase);time+=0.35;
            let silent=p.read_rgba(&p.canvas)?;
            p.render_voice(dark,time,phase,1.,1.);let loud=p.read_rgba(&p.canvas)?;
            ensure(silent.chunks_exact(4).all(|c|c[..3].iter().all(|v|*v<=c[3])),"Current voice state is not premultiplied")?;
            if phase==Phase::Listening {ensure(delta(&silent,&loud)>20,"Current listening state ignores audio level")?;}
            else {ensure(silent==loud,"Audio level changes a non-listening adaptive state")?;}
            if let Some(d)=directory {
                let path=d.join("current-states");std::fs::create_dir_all(&path)?;
                crate::png::write(&path.join(format!("k{kind}-{}-{}.png",if dark{"dark"}else{"light"},phase as u32)),p.raw.width,p.raw.height,&p.composite_fixture()?)?;
            }
            states+=1;
        }
        p.render_voice(dark,time+0.01,Phase::Hidden,0.,0.);
        ensure(p.read_rgba(&p.canvas)?.iter().all(|c|*c==0),"Current hidden state left a visible canvas")?;
        if kind==3 {
            p.set_voice_mask(&vec![0u8;162*39])?;
            frames(&p,dark,time+0.01,time+0.5,Phase::Activating);let time=time+0.5;
            let real=p.read_rgba(&p.canvas)?;
            let source=format!("#define LIQUID_VOICE_CONTENT 1\n#define LIQUID_TEST_NO_LENS 1\n{}\n{}\n{}\n{}",include_str!("glass.hlsl"),include_str!("voice_content.hlsl"),include_str!("liquid.hlsl"),include_str!("adaptive.hlsl"));
            let bytes=compile_source(&source,s!("adaptive_material_ps"),s!("ps_5_0"))?;
            let mut no_lens=None;p.device.CreatePixelShader(blob_bytes(&bytes),None,Some(&mut no_lens))?;
            let normal=p.shader.clone();p.shader=no_lens.ok_or("No lens-ablation shader")?;
            p.render_voice(dark,time,Phase::Activating,0.,1.);let flat=p.read_rgba(&p.canvas)?;
            let differences=delta(&real,&flat);
            ensure(differences>300,"Current adaptive path no longer refracts the grid")?;
            ensure(real.chunks_exact(4).zip(flat.chunks_exact(4)).all(|(a,b)|a[3]==b[3]),"Lens ablation changed geometry/alpha")?;
            p.shader=normal;p.render_voice(dark,time,Phase::Activating,0.,1.);
            ensure(real==p.read_rgba(&p.canvas)?,"Restoring current lens at the same timestamp changed history")?;
            changed_by_lens+=differences;
        }
    }}
    // Rapid reversals at 33 ms intervals: the CURRENT green texture must not wait
    // for previous white/black colors to fade. Only material coefficients may lag.
    let p=AdaptivePipeline::new_voice(device.clone(),context.clone(),162,39,&vec![0u8;162*39])?;
    upload_fixture_checked(&p,&input(p.raw.width,p.raw.height,0))?;frames(&p,false,0.,0.8,Phase::Activating);
    for (n,kind) in [2u32,0,2,1,2,0,2,1,2].iter().enumerate() {
        upload_fixture_checked(&p,&input(p.raw.width,p.raw.height,*kind))?;
        p.render_voice(false,0.8+(n+1) as f32/30.,Phase::Activating,0.,1.);
        if *kind==2 {
            let out=p.read_rgba(&p.output)?;let at=((19*162+81)*4) as usize;
            ensure(out[at+1] as i32-out[at] as i32>50,"Rapid reversal carried stale scene color")?;
        }
    }
    // Optional review export: every frame is computed by both HLSL renderers.
    // No interpolated/keyframe-invented shader images and no captured user pixels.
    // Times are synthetic 60 Hz inputs, NOT measured screen FPS or latency.
    if let Some(d)=directory {
        let mask=crate::voice_window::content_mask(162,39,1.5,Phase::Optimizing)?;
        for dark in [false,true] {
            let p=AdaptivePipeline::new_voice(device.clone(),context.clone(),162,39,&mask)?;
            let baseline=Pipeline::new_voice(device.clone(),context.clone(),162,39,&mask)?;
            let path=d.join(if dark{"film-dark"}else{"film-light"});std::fs::create_dir_all(&path)?;
            let mut previous=None;let mut rows=String::from("frame,time_seconds,generated_background\n");
            for frame in 0..240 {
                let time=frame as f32/60.;
                let kind=if frame<60{0}else if frame<102{2}else if frame<144{0}else if frame<186{1}else{0};
                if previous!=Some(kind) {
                    let source=input(p.raw.width,p.raw.height,kind);
                    let baseline_source=input(baseline.raw.width,baseline.raw.height,kind);
                    upload_fixture_checked(&p,&source)?;upload_fixture_checked(&baseline,&baseline_source)?;previous=Some(kind);
                }
                p.render_voice(dark,time,Phase::Optimizing,0.,1.);baseline.render_voice(dark,time,Phase::Optimizing,0.,1.);
                let current=p.composite_fixture()?;
                crate::png::write(&path.join(format!("adaptive-{frame:03}.png")),p.raw.width,p.raw.height,&current)?;
                baseline.snapshot(&path,frame+1)?;
                rows.push_str(&format!("{frame},{time:.6},{kind}\n"));
            }
            std::fs::write(path.join("timeline.csv"),rows)?;
        }
    }
    let message=format!("[adaptive-current] PASS; {states} current production state/theme/source cases; current lens ablation changed {changed_by_lens} pixels with identical alpha; actual level sensitivity, hidden release pixels, same-frame restore, rapid 33ms reversals; optional film frames are synthetic 60Hz HLSL outputs, not display FPS");
    if let Some(d)=directory {std::fs::write(d.join("current-contracts.txt"),&message)?;}
    eprintln!("{message}");Ok(())
}

#[cfg(test)]mod tests {
    #[test]fn current_adaptive_voice_states_and_review_frames(){unsafe{
        let dir=std::env::var_os("GLASS_MATERIAL_FIXTURES").map(|v|std::path::PathBuf::from(v).with_extension("adaptive-current"));
        if let Some(d)=&dir {std::fs::create_dir_all(d).unwrap();}
        super::verify(dir.as_deref()).unwrap();
    }}
}
