//! Real shared renderer, generated inputs only. Safe in the production EXE's
//! --liquid-glass-self-test path: no HWND, desktop duplication, audio or network.
use crate::{AppResult,ensure,gpu::{self,Pipeline},voice_model::Phase};
use std::path::Path;

/// This boundary deliberately accepts bytes, not an unconstrained integer Vec.
/// Validate the actual BGRA upload by reading it back as RGBA before interpreting
/// shader results. This helper is only used by generated-fixture checks.
pub(crate) unsafe fn upload_fixture_checked(p:&Pipeline,bgra:&[u8])->AppResult<()> {
    ensure(bgra.len()==(p.raw.width*p.raw.height*4) as usize,"Invalid synthetic BGRA byte length")?;
    p.context.UpdateSubresource(&p.raw.texture,0,None,bgra.as_ptr().cast(),p.raw.width*4,0);
    let rgba=p.read_rgba(&p.raw)?;
    ensure(rgba.len()==bgra.len(),"Synthetic upload readback size differs")?;
    for (actual,expected) in rgba.chunks_exact(4).zip(bgra.chunks_exact(4)) {
        ensure(actual==[expected[2],expected[1],expected[0],expected[3]],
            "Synthetic background upload changed channels or row pitch")?;
    }
    p.prepare();
    Ok(())
}

/// Product colors, not a palette invented for the glass preview. Matches
/// clients/desktop-client/src/main.rs::lerp_rgb at dfb2b6d / original recording UI.
fn original_wave_rgb(index:usize)->[u8;3] {
    let start=[67.0f32,222.0,210.0];
    let end=[100.0f32,141.0,255.0];
    let t=index as f32/19.0;
    std::array::from_fn(|c|(start[c]+(end[c]-start[c])*t).round() as u8)
}

/// Check each fully covered stroke core, not a whole-image chroma statistic
/// (a colored background must not make a monochrome waveform pass).
unsafe fn verify_listening_palette()->AppResult<()> {
    let (device,context)=gpu::create_device(None)?;
    let mut cases=0usize;
    let mut checked=0usize;
    let mut max_error=0u8;
    for (w,h) in [(108u32,26u32),(162,39),(324,78)] {
        let p=Pipeline::new_voice(device.clone(),context.clone(),w,h,&vec![0u8;(w*h) as usize])?;
        let rw=p.raw.width;let rh=p.raw.height;
        for dark in [false,true] { for kind in 0..5 {
            let mut input=vec![0u8;(rw*rh*4) as usize];
            for y in 0..rh { for x in 0..rw {
                let bgra:[u8;4]=match kind {
                    0=>[0,0,0,255],
                    1=>[255,255,255,255],
                    2=>[128,128,128,255],
                    3=>if x<rw/2{[64,48,220,255]}else{[224,112,32,255]},
                    _=>if x%24<2||y%16<2{[16,16,16,255]}else{[235,235,235,255]},
                };
                input[((y*rw+x)*4) as usize..][..4].copy_from_slice(&bgra);
            }}
            upload_fixture_checked(&p,&input)?;
            for time in [2.0f32,2.4] { for level in [0.0f32,1.0] {
                p.render_voice(dark,time,Phase::Listening,level,1.0);
                let pixels=p.read_rgba(&p.output)?;
                let scale=h as f32/26.0;
                for index in 0..20 {
                    let center=(w as f32-78.0*scale)*0.5+(index as f32*4.0+1.0)*scale;
                    let x=center.floor() as u32;let y=h/2;
                    ensure(x<w,"Waveform probe outside output")?;
                    let actual=&pixels[((y*w+x)*4) as usize..][..4];
                    let expected=original_wave_rgb(index);
                    ensure(actual[3]==255,"Waveform core is not fully covered")?;
                    for channel in 0..3 {
                        let error=actual[channel].abs_diff(expected[channel]);
                        max_error=max_error.max(error);
                        ensure(error<=1,"Listening waveform lost original cyan-to-blue sRGB palette")?;
                    }
                    checked+=1;
                }
                cases+=1;
            }}
        }}
    }
    ensure(cases==120 && checked==2400,"Incomplete waveform palette coverage")?;
    eprintln!("[voice-waveform-palette] PASS; original=#43DED2->#648DFF; {cases} generated size/theme/background/level/time cases; {checked} stroke-core pixels; max RGB byte error={max_error}; no capture/audio/network");
    Ok(())
}

pub(crate) unsafe fn verify(directory:Option<&Path>)->AppResult<()> {
    let (d,c)=gpu::create_device(None)?;
    let (w,h)=(162,39);
    let mask=crate::voice_window::content_mask(w,h,1.5,Phase::Optimizing)?;
    let p=Pipeline::new_voice(d.clone(),c.clone(),w,h,&mask)?;
    let rw=p.raw.width;let rh=p.raw.height;
    if let Some(dir)=directory{std::fs::create_dir_all(dir)?;}
    let mut number=0;
    for dark in [false,true]{for kind in 0..3{
        let mut input:Vec<u8>=vec![0u8;(rw*rh*4) as usize];
        for y in 0..rh{for x in 0..rw{
            let bgra:[u8;4]=match kind{
                0=>[248,248,248,255],
                1=>if x<rw/2{[64,48,220,255]}else{[224,112,32,255]},
                _=>if x%24<2||y%16<2{[16,16,16,255]}else{[235,235,235,255]},
            };input[((y*rw+x)*4) as usize..][..4].copy_from_slice(&bgra);
        }}
        upload_fixture_checked(&p,&input)?;
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
    verify_listening_palette()?;
    eprintln!("[voice-render-contract] PASS; 30 generated state/theme/background cases; BGRA input readback exact; actual audio-level control; close alpha zero; no capture/audio/network");
    Ok(())
}
#[cfg(test)]mod tests{
    use super::*;
    #[test]fn all_production_states_use_actual_optics(){unsafe{
        let dir=std::env::var_os("GLASS_MATERIAL_FIXTURES").map(|p|std::path::PathBuf::from(p).with_extension("voice"));
        verify(dir.as_deref()).unwrap();
    }}
}
