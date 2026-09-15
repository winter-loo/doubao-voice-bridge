//! Real shared renderer, generated inputs only. Safe in the production EXE's
//! --liquid-glass-self-test path: no HWND, desktop duplication, audio or network.
use crate::{AppResult,ensure,gpu::{self,Pipeline},voice_model::Phase};
use std::path::Path;
pub(crate) unsafe fn verify(directory:Option<&Path>)->AppResult<()> {
    let (d,c)=gpu::create_device(None)?;
    let (w,h)=(162,39);
    let mask=crate::voice_window::content_mask(w,h,1.5,Phase::Optimizing)?;
    let p=Pipeline::new_voice(d.clone(),c.clone(),w,h,&mask)?;
    let rw=p.raw.width;let rh=p.raw.height;
    if let Some(dir)=directory{std::fs::create_dir_all(dir)?;}
    let mut number=0;
    for dark in [false,true]{for kind in 0..3{
        let mut input=vec![0;(rw*rh*4) as usize];
        for y in 0..rh{for x in 0..rw{
            let rgb=match kind{
                0=>[248,248,248,255],
                1=>if x<rw/2{[64,48,220,255]}else{[224,112,32,255]},
                _=>if x%24<2||y%16<2{[16,16,16,255]}else{[235,235,235,255]},
            };input[((y*rw+x)*4) as usize..][..4].copy_from_slice(&rgb);
        }}
        p.context.UpdateSubresource(&p.raw.texture,0,None,input.as_ptr().cast(),rw*4,0);p.prepare();
        for phase in [Phase::Activating,Phase::Listening,Phase::Optimizing,Phase::Completed,Phase::Failed]{
            let mask=crate::voice_window::content_mask(w,h,1.5,phase)?;p.set_voice_mask(&mask)?;
            let t=2.+number as f32;
            p.render_voice(dark,t,phase,0.,1.);let silent=p.read_rgba(&p.output)?;
            ensure(silent.chunks_exact(4).all(|v|v[..3].iter().all(|b|*b<=v[3])),"Voice output is not premultiplied")?;
            p.render_voice(dark,t,phase,1.,1.);let loud=p.read_rgba(&p.output)?;
            if phase==Phase::Listening{ensure(silent!=loud,"Listening waveform ignores actual audio level")?;}
            else{ensure(silent==loud,"Non-recording UI is driven by microphone level")?;}
            number+=1;
            if let Some(dir)=directory{p.snapshot(dir,number)?;}
        }
    }}
    p.render_voice(false,100.,Phase::Hidden,0.,0.);
    ensure(p.read_rgba(&p.output)?.iter().all(|v|*v==0),"Closed voice overlay left visible pixels")?;
    eprintln!("[voice-render-contract] PASS; 30 generated state/theme/background cases; actual audio-level control; close alpha zero; no capture/audio/network");
    Ok(())
}
#[cfg(test)]mod tests{
    use super::*;
    #[test]fn all_production_states_use_actual_optics(){unsafe{
        let dir=std::env::var_os("GLASS_MATERIAL_FIXTURES").map(|p|std::path::PathBuf::from(p).with_extension("voice"));
        verify(dir.as_deref()).unwrap();
    }}
}
