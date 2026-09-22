//! Fast generated-fixture Liquid Glass playground. No GPUI, microphone, network,
//! desktop capture, startup mutation, singleton, or production executable replacement.
use crate::{
    ensure,
    gpu::{self, AdaptivePipeline, AdaptivePresenter, MaterialTuning},
    voice_model::Phase,
    voice_render_checks::upload_fixture_checked,
    voice_window,
    white_text_checks,
    AppResult, PlaygroundReferenceImage,
};
use std::{
    cell::RefCell,
    mem::size_of,
    path::{Path, PathBuf},
    time::{Duration, Instant, SystemTime},
};
use windows::{
    core::{w, PCWSTR},
    Win32::{
        Foundation::{HINSTANCE, HWND, LPARAM, LRESULT, POINT, RECT, WPARAM},
        Graphics::{
            Dxgi::{CreateDXGIFactory1, IDXGIFactory2},
            Gdi::{
                BeginPaint, ClientToScreen, EndPaint, InvalidateRect, PatBlt, StretchDIBits,
                UpdateWindow, ValidateRect, BITMAPINFO, BITMAPINFOHEADER, BI_RGB, DIB_RGB_COLORS,
                PAINTSTRUCT, SRCCOPY, WHITENESS,
            },
        },
        System::{
            Com::{CoInitializeEx, CoUninitialize, COINIT_APARTMENTTHREADED},
            LibraryLoader::GetModuleHandleW,
        },
        UI::{HiDpi::*, WindowsAndMessaging::*},
    },
};

const W: u32 = 324;
const H: u32 = 78;
const SCALE: f32 = H as f32 / 26.0;
const REFERENCE_GAP: u32 = 16;

// Interactive reference composition: two circular controls and one center pill.
// These dimensions exist only in the visual playground; production geometry is unchanged.
const SCENE_W: u32 = 640;
const SCENE_H: u32 = 210;
const GLASS_H: u32 = 78;
const LEFT_W: u32 = 78;
const CENTER_W: u32 = 282;
const RIGHT_W: u32 = 78;
const LEFT_X: i32 = 70;
const CENTER_X: i32 = 178;
const RIGHT_X: i32 = 490;
const GLASS_Y: i32 = 66;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum GlassRole { Left, Center, Right }

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Fixture { WhiteText, White, Dark, Green, Colorful, AppleOptics }
impl Fixture {
    fn name(self) -> &'static str {
        match self {
            Self::WhiteText => "white+black-text",
            Self::White => "white",
            Self::Dark => "dark",
            Self::Green => "green",
            Self::Colorful => "colorful",
            Self::AppleOptics => "apple-optics",
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum PreviewState { Steady, Listening, Activating, Optimizing }
impl PreviewState {
    fn name(self) -> &'static str {
        match self {
            Self::Steady => "steady",
            Self::Listening => "listening",
            Self::Activating => "activating",
            Self::Optimizing => "optimizing-text",
        }
    }
    fn phase(self) -> Phase {
        match self {
            Self::Steady | Self::Activating => Phase::Activating,
            Self::Listening => Phase::Listening,
            Self::Optimizing => Phase::Optimizing,
        }
    }
    fn level(self) -> f32 { if self == Self::Listening { 0.72 } else { 0.0 } }
}

#[derive(Default)]
struct PaintState {
    bgra: Vec<u8>,
    width: u32,
    height: u32,
    apple: Option<PlaygroundReferenceImage>,
    show_apple: bool,
}
thread_local! {
    static PAINT: RefCell<PaintState> = RefCell::new(PaintState::default());
    static KEYS: RefCell<Vec<usize>> = RefCell::new(Vec::new());
}

unsafe extern "system" fn host_proc(hwnd:HWND,msg:u32,wp:WPARAM,lp:LPARAM)->LRESULT {
    match msg {
        WM_ERASEBKGND => LRESULT(1),
        WM_PAINT => {
            let mut ps=PAINTSTRUCT::default();
            let hdc=BeginPaint(hwnd,&mut ps);
            PAINT.with(|state|{
                let state=state.borrow();
                if !state.bgra.is_empty() {
                    let info=BITMAPINFO {
                        bmiHeader: BITMAPINFOHEADER {
                            biSize:size_of::<BITMAPINFOHEADER>() as u32,
                            biWidth:state.width as i32,
                            biHeight:-(state.height as i32),
                            biPlanes:1,
                            biBitCount:32,
                            biCompression:BI_RGB.0,
                            ..Default::default()
                        },
                        ..Default::default()
                    };
                    let _=StretchDIBits(
                        hdc,0,0,state.width as i32,state.height as i32,
                        0,0,state.width as i32,state.height as i32,
                        Some(state.bgra.as_ptr().cast()),&info,DIB_RGB_COLORS,SRCCOPY
                    );
                    if state.show_apple {
                        let panel_x=(state.width+REFERENCE_GAP) as i32;
                        let _=PatBlt(
                            hdc,state.width as i32,0,
                            (REFERENCE_GAP+state.width) as i32,state.height as i32,WHITENESS
                        );
                        if let Some(reference)=&state.apple {
                            let scale=(state.width as f32/reference.width as f32)
                                .min(state.height as f32/reference.height as f32);
                            let dw=(reference.width as f32*scale).round().max(1.0) as i32;
                            let dh=(reference.height as f32*scale).round().max(1.0) as i32;
                            let dx=panel_x+(state.width as i32-dw)/2;
                            let dy=(state.height as i32-dh)/2;
                            let reference_info=BITMAPINFO {
                                bmiHeader:BITMAPINFOHEADER {
                                    biSize:size_of::<BITMAPINFOHEADER>() as u32,
                                    biWidth:reference.width as i32,
                                    biHeight:-(reference.height as i32),
                                    biPlanes:1,
                                    biBitCount:32,
                                    biCompression:BI_RGB.0,
                                    ..Default::default()
                                },
                                ..Default::default()
                            };
                            let _=StretchDIBits(
                                hdc,dx,dy,dw,dh,
                                0,0,reference.width as i32,reference.height as i32,
                                Some(reference.bgra.as_ptr().cast()),&reference_info,DIB_RGB_COLORS,SRCCOPY
                            );
                        }
                    }
                }
            });
            let _=EndPaint(hwnd,&ps);
            LRESULT(0)
        }
        WM_KEYDOWN => { KEYS.with(|keys|keys.borrow_mut().push(wp.0)); LRESULT(0) }
        WM_CLOSE => { let _=DestroyWindow(hwnd); LRESULT(0) }
        WM_DESTROY => { PostQuitMessage(0); LRESULT(0) }
        _ => DefWindowProcW(hwnd,msg,wp,lp),
    }
}
unsafe extern "system" fn overlay_proc(hwnd:HWND,msg:u32,wp:WPARAM,lp:LPARAM)->LRESULT {
    match msg {
        WM_NCHITTEST => LRESULT(HTTRANSPARENT as isize),
        WM_MOUSEACTIVATE => LRESULT(MA_NOACTIVATE as isize),
        WM_ERASEBKGND => LRESULT(1),
        WM_PAINT => { let _=ValidateRect(Some(hwnd),None); LRESULT(0) }
        _ => DefWindowProcW(hwnd,msg,wp,lp),
    }
}

struct ComGuard;
impl Drop for ComGuard { fn drop(&mut self){unsafe{CoUninitialize();}} }
struct Window(HWND);
impl Drop for Window { fn drop(&mut self){unsafe{if IsWindow(Some(self.0)).as_bool(){let _=DestroyWindow(self.0);}}} }

fn shifted_mask(source:&[u8],w:u32,h:u32,dx:i32,dy:i32)->Vec<u8> {
    let mut out=vec![0u8;(w*h) as usize];
    for y in 0..h as i32 { for x in 0..w as i32 {
        let sx=x-dx;let sy=y-dy;
        if sx>=0&&sy>=0&&sx<w as i32&&sy<h as i32 {
            out[(y as u32*w+x as u32) as usize]=source[(sy as u32*w+sx as u32) as usize];
        }
    }}
    out
}
fn merge_mask(dst:&mut [u8],src:&[u8]) {
    for (d,s) in dst.iter_mut().zip(src) {*d=(*d).max(*s);}
}
unsafe fn label_mask(w:u32,h:u32,scale:f32,label:&str,dx:i32,dy:i32)->AppResult<Vec<u8>> {
    let base=crate::desktop::text_mask_for_label(w,h,scale,false,label,false)?;
    Ok(shifted_mask(&base,w,h,dx,dy))
}

fn point_segment_distance(px:f32,py:f32,ax:f32,ay:f32,bx:f32,by:f32)->f32 {
    let vx=bx-ax;let vy=by-ay;let wx=px-ax;let wy=py-ay;
    let vv=vx*vx+vy*vy;let t=if vv>0.0{((wx*vx+wy*vy)/vv).clamp(0.0,1.0)}else{0.0};
    let dx=px-(ax+t*vx);let dy=py-(ay+t*vy);(dx*dx+dy*dy).sqrt()
}

fn arrow_mask(w:u32,h:u32)->Vec<u8> {
    let mut out=vec![0u8;(w*h) as usize];let hf=h as f32;
    let tip=(w as f32*0.34,hf*0.50);let upper=(w as f32*0.63,hf*0.25);let lower=(w as f32*0.63,hf*0.75);
    let half=hf*0.043;
    for y in 0..h {for x in 0..w {
        let px=x as f32+0.5;let py=y as f32+0.5;
        let d=point_segment_distance(px,py,tip.0,tip.1,upper.0,upper.1)
            .min(point_segment_distance(px,py,tip.0,tip.1,lower.0,lower.1));
        out[(y*w+x) as usize]=((half+0.75-d).clamp(0.0,1.0)*255.0).round() as u8;
    }}out
}

fn ellipsis_mask(w:u32,h:u32)->Vec<u8> {
    let mut out=vec![0u8;(w*h) as usize];let r=h as f32*0.055;let cy=h as f32*0.50;
    for y in 0..h {for x in 0..w {
        let px=x as f32+0.5;let py=y as f32+0.5;let mut d=1e9f32;
        for cx in [w as f32*0.34,w as f32*0.50,w as f32*0.66] {
            d=d.min(((px-cx)*(px-cx)+(py-cy)*(py-cy)).sqrt()-r);
        }
        out[(y*w+x) as usize]=((0.75-d).clamp(0.0,1.0)*255.0).round() as u8;
    }}out
}

unsafe fn role_mask(role:GlassRole,state:PreviewState,w:u32,h:u32)->AppResult<Vec<u8>> {
    match role {
        GlassRole::Left => Ok(arrow_mask(w,h)),
        GlassRole::Right => Ok(ellipsis_mask(w,h)),
        GlassRole::Center if state==PreviewState::Steady => {
            let mut out=vec![0u8;(w*h) as usize];
            let today=label_mask(w,h,1.62,"\u{4eca}\u{5929}",0,-13)?;
            let time=label_mask(w,h,1.20,"11:08",0,15)?;
            merge_mask(&mut out,&today);merge_mask(&mut out,&time);Ok(out)
        }
        GlassRole::Center => voice_window::content_mask(w,h,h as f32/26.0,state.phase()),
    }
}

fn crop_scene(scene:&[u8],scene_w:u32,scene_h:u32,x:i32,y:i32,w:u32,h:u32)->AppResult<Vec<u8>> {
    ensure(scene.len()==(scene_w*scene_h*4) as usize,"Invalid reference scene byte length")?;
    // AppleOptics is generated on white paper. The visual/capture margin may be
    // wider than the fixture scene near an edge, so preserve the canonical body
    // coordinates and synthesize only the off-scene paper pixels as opaque white.
    // This is playground fixture plumbing; production desktop capture never uses it.
    let mut out=vec![255u8;(w*h*4) as usize];
    for row in 0..h as i32 {for col in 0..w as i32 {
        let sx=x+col;let sy=y+row;
        if sx<0||sy<0||sx>=scene_w as i32||sy>=scene_h as i32 {continue;}
        let src=((sy as u32*scene_w+sx as u32)*4) as usize;
        let dst=((row as u32*w+col as u32)*4) as usize;
        out[dst..dst+4].copy_from_slice(&scene[src..src+4]);
    }}
    Ok(out)
}

unsafe fn upload_scene_crop(
    reference:&AdaptivePipeline,candidate:&AdaptivePipeline,scene:&[u8],scene_w:u32,scene_h:u32,
    body_x:i32,body_y:i32,
)->AppResult<()> {
    ensure(reference.raw.width==candidate.raw.width&&reference.raw.height==candidate.raw.height,
        "A/B raw crop geometry differs")?;
    ensure(reference.padding==candidate.padding,"A/B raw crop padding differs")?;
    let pad=reference.padding as i32;
    let crop=crop_scene(scene,scene_w,scene_h,body_x-pad,body_y-pad,reference.raw.width,reference.raw.height)?;
    upload_fixture_checked(reference,&crop)?;upload_fixture_checked(candidate,&crop)
}

unsafe fn set_pair_mask(reference:&AdaptivePipeline,candidate:&AdaptivePipeline,mask:&[u8])->AppResult<()> {
    reference.set_voice_mask(mask)?;candidate.set_voice_mask(mask)
}
fn fixture_pixels(kind:Fixture,w:u32,h:u32)->AppResult<Vec<u8>> {
    if kind==Fixture::WhiteText {
        return unsafe { white_text_checks::generated_white_text_fixture(w,h) };
    }
    if kind==Fixture::AppleOptics {
        // Canonical playground scene: white paper with repeated oversized black
        // "缂傚倷缍€閸涱垱鏆伴柡澶嗘櫃缁瑥螞? text. The same generated scene is cropped into all three glass
        // pipelines, so cap/body refraction sees spatially coherent source pixels.
        let line1=unsafe { crate::desktop::text_mask_for_label_weight(
            w,h,3.82,false,"\u{7ee7}\u{7eed}\u{8ffd}\u{95ee}  \u{7ee7}\u{7eed}\u{8ffd}\u{95ee}  \u{7ee7}\u{7eed}\u{8ffd}\u{95ee}",false,650
        )? };
        let line2=unsafe { crate::desktop::text_mask_for_label_weight(
            w,h,3.42,false,"\u{7ee7}\u{7eed}\u{8ffd}\u{95ee}  \u{7ee7}\u{7eed}\u{8ffd}\u{95ee}",false,650
        )? };
        let line1=shifted_mask(&line1,w,h,-8,0);
        let line2=shifted_mask(&line2,w,h,32,78);
        let mut coverage=line1;merge_mask(&mut coverage,&line2);
        let mut bgra=vec![0u8;(w*h*4) as usize];
        for (index,alpha) in coverage.into_iter().enumerate() {
            let value=255u8.saturating_sub(alpha);
            bgra[index*4..index*4+4].copy_from_slice(&[value,value,value,255]);
        }
        return Ok(bgra);
    }
    let mut out=vec![0u8;(w*h*4) as usize];
    for y in 0..h { for x in 0..w {
        let c:[u8;4]=match kind {
            Fixture::White => [255,255,255,255],
            Fixture::Dark => [8,8,8,255],
            Fixture::Green => [86,183,9,255],
            Fixture::Colorful => if x<w/2 {
                if y%32<16 {[64,48,220,255]} else {[32,180,245,255]}
            } else if y%32<16 {[224,112,32,255]} else {[220,55,150,255]},
            Fixture::WhiteText | Fixture::AppleOptics => unreachable!(),
        };
        out[((y*w+x)*4) as usize..][..4].copy_from_slice(&c);
    }}
    Ok(out)
}
fn mask_for(state:PreviewState)->AppResult<Vec<u8>> {
    if state==PreviewState::Steady { return Ok(vec![0u8;(W*H) as usize]); }
    unsafe { voice_window::content_mask(W,H,SCALE,state.phase()) }
}
unsafe fn set_fixture(
    host:HWND,
    reference:&AdaptivePipeline,
    candidate:&AdaptivePipeline,
    fixture:Fixture,
)->AppResult<()> {
    let pixels=fixture_pixels(fixture,reference.raw.width,reference.raw.height)?;
    upload_fixture_checked(reference,&pixels)?;
    upload_fixture_checked(candidate,&pixels)?;
    PAINT.with(|state|{
        let mut state=state.borrow_mut();
        state.bgra=pixels;
        state.width=reference.raw.width;
        state.height=reference.raw.height;
    });
    let _=InvalidateRect(Some(host),None,false);
    Ok(())
}
unsafe fn set_state(reference:&AdaptivePipeline,candidate:&AdaptivePipeline,state:PreviewState)->AppResult<()> {
    let mask=mask_for(state)?;
    reference.set_voice_mask(&mask)?;
    candidate.set_voice_mask(&mask)?;
    Ok(())
}
unsafe fn reset_pair(reference:&AdaptivePipeline,candidate:&AdaptivePipeline) {
    reference.reset_fixture_state();
    candidate.reset_fixture_state();
}
unsafe fn position_overlay_at(host:HWND,overlay:HWND,pipe:&AdaptivePipeline,body_x:i32,body_y:i32)->AppResult<()> {
    let mut p=POINT::default();
    ensure(ClientToScreen(host,&mut p).as_bool(),"Cannot map playground client to screen coordinates")?;
    SetWindowPos(
        overlay,None,
        p.x+body_x-pipe.layout.margin as i32,p.y+body_y-pipe.layout.margin as i32,
        pipe.canvas.width as i32,pipe.canvas.height as i32,
        SWP_NOACTIVATE|SWP_SHOWWINDOW|SWP_NOZORDER
    )?;
    Ok(())
}
unsafe fn position_overlay(host:HWND,overlay:HWND,pipe:&AdaptivePipeline)->AppResult<()> {
    ensure(pipe.layout.margin<=pipe.padding,"Playground canvas margin exceeds generated fixture padding")?;
    position_overlay_at(host,overlay,pipe,pipe.padding as i32,pipe.padding as i32)
}
unsafe fn set_apple_scene(
    host:HWND,
    left_reference:&AdaptivePipeline,left_candidate:&AdaptivePipeline,
    center_reference:&AdaptivePipeline,center_candidate:&AdaptivePipeline,
    right_reference:&AdaptivePipeline,right_candidate:&AdaptivePipeline,
)->AppResult<()> {
    let scene=fixture_pixels(Fixture::AppleOptics,SCENE_W,SCENE_H)?;
    upload_scene_crop(left_reference,left_candidate,&scene,SCENE_W,SCENE_H,LEFT_X,GLASS_Y)?;
    upload_scene_crop(center_reference,center_candidate,&scene,SCENE_W,SCENE_H,CENTER_X,GLASS_Y)?;
    upload_scene_crop(right_reference,right_candidate,&scene,SCENE_W,SCENE_H,RIGHT_X,GLASS_Y)?;
    PAINT.with(|state|{
        let mut state=state.borrow_mut();state.bgra=scene;state.width=SCENE_W;state.height=SCENE_H;
    });
    let _=InvalidateRect(Some(host),None,false);Ok(())
}
unsafe fn set_apple_state(
    left_reference:&AdaptivePipeline,left_candidate:&AdaptivePipeline,
    center_reference:&AdaptivePipeline,center_candidate:&AdaptivePipeline,
    right_reference:&AdaptivePipeline,right_candidate:&AdaptivePipeline,
    state:PreviewState,
)->AppResult<()> {
    set_pair_mask(left_reference,left_candidate,&role_mask(GlassRole::Left,state,LEFT_W,GLASS_H)?)?;
    set_pair_mask(center_reference,center_candidate,&role_mask(GlassRole::Center,state,CENTER_W,GLASS_H)?)?;
    set_pair_mask(right_reference,right_candidate,&role_mask(GlassRole::Right,state,RIGHT_W,GLASS_H)?)?;
    Ok(())
}
unsafe fn reset_all(pipes:&[&AdaptivePipeline]) {
    for pipe in pipes { pipe.reset_fixture_state(); }
}

unsafe fn composite_canvas_into_scene(
    scene:&mut [u8],scene_w:u32,scene_h:u32,pipe:&AdaptivePipeline,body_x:i32,body_y:i32,
)->AppResult<()> {
    ensure(scene.len()==(scene_w*scene_h*4) as usize,"Invalid Apple scene RGBA byte length")?;
    let layer=pipe.read_rgba(&pipe.canvas)?;
    ensure(layer.len()==(pipe.canvas.width*pipe.canvas.height*4) as usize,"Invalid glass canvas readback")?;
    let ox=body_x-pipe.layout.margin as i32;let oy=body_y-pipe.layout.margin as i32;
    for y in 0..pipe.canvas.height as i32 {for x in 0..pipe.canvas.width as i32 {
        let dx=ox+x;let dy=oy+y;if dx<0||dy<0||dx>=scene_w as i32||dy>=scene_h as i32{continue;}
        let si=((y as u32*pipe.canvas.width+x as u32)*4) as usize;
        let di=((dy as u32*scene_w+dx as u32)*4) as usize;
        let a=layer[si+3] as u32;
        for c in 0..3 {scene[di+c]=(layer[si+c] as u32+(scene[di+c] as u32*(255-a)+127)/255).min(255) as u8;}
        scene[di+3]=255;
    }}
    Ok(())
}
unsafe fn write_apple_scene_snapshot(
    path:&Path,left:&AdaptivePipeline,center:&AdaptivePipeline,right:&AdaptivePipeline,
)->AppResult<()> {
    let mut scene=fixture_pixels(Fixture::AppleOptics,SCENE_W,SCENE_H)?;
    for pixel in scene.chunks_exact_mut(4){pixel.swap(0,2);} // generated fixture BGRA -> RGBA
    composite_canvas_into_scene(&mut scene,SCENE_W,SCENE_H,left,LEFT_X,GLASS_Y)?;
    composite_canvas_into_scene(&mut scene,SCENE_W,SCENE_H,center,CENTER_X,GLASS_Y)?;
    composite_canvas_into_scene(&mut scene,SCENE_W,SCENE_H,right,RIGHT_X,GLASS_Y)?;
    crate::png::write_replace(path,SCENE_W,SCENE_H,&scene)?;Ok(())
}

fn title(
    fixture:Fixture,state:PreviewState,show_production:bool,dark:bool,
    shader_error:Option<&str>,reload_ms:Option<u128>,
    tuning:&MaterialTuning,selected:usize,tuning_error:Option<&str>,
    show_apple:bool,apple_error:Option<&str>,
)->String {
    let shader_status=if let Some(error)=shader_error {
        let first=error.lines().next().unwrap_or("compile error");
        let short:String=first.chars().take(96).collect();
        format!("HLSL ERROR: {short}")
    } else if let Some(ms)=reload_ms {
        format!("HLSL last-good {ms}ms")
    } else {
        "HLSL embedded last-good".to_string()
    };
    let tuning_status=if let Some(error)=tuning_error {
        let first=error.lines().next().unwrap_or("tuning error");
        let short:String=first.chars().take(72).collect();
        format!("TUNE ERROR: {short}")
    } else {
        format!("{}={:.3}",tuning_name(selected),tuning_value(tuning,selected))
    };
    let apple_status=if let Some(error)=apple_error {
        let first=error.lines().next().unwrap_or("reference error");
        let short:String=first.chars().take(64).collect();
        format!("Apple visual-only ERROR: {short}")
    } else if show_apple {
        "Apple visual-only ON".to_string()
    } else {
        "Apple visual-only off".to_string()
    };
    format!(
        "Liquid Glass Playground | {} | {} | {} | {} | {} | {} | {} | 1..6 fixtures  S/Space/A/T states  Tab A/B  C capture  R Apple ref  D theme  Left/Right param  Up/Down value  P reset  V save  Esc close",
        if show_production{"A production"}else{"B candidate"},
        fixture.name(),state.name(),if dark{"dark"}else{"light"},shader_status,tuning_status,apple_status
    )
}
unsafe fn set_title(hwnd:HWND,text:&str)->AppResult<()> {
    let wide:Vec<u16>=text.encode_utf16().chain(std::iter::once(0)).collect();
    SetWindowTextW(hwnd,PCWSTR(wide.as_ptr()))?;
    Ok(())
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct FileStamp { modified:Option<SystemTime>, len:u64 }
fn file_stamp(path:&PathBuf)->Option<FileStamp> {
    let metadata=std::fs::metadata(path).ok()?;
    Some(FileStamp{modified:metadata.modified().ok(),len:metadata.len()})
}

const TUNING_COUNT:usize=16;
fn tuning_name(index:usize)->&'static str {
    [
        "scene_guard","local_guard","scene_veil","local_veil",
        "neutral_low","neutral_high","neutral_chroma_low","neutral_chroma_high",
        "detail_low","detail_high","complexity_low","complexity_high",
        "frost_strength","milkiness","interior_start","interior_full",
    ][index%TUNING_COUNT]
}
fn tuning_value(t:&MaterialTuning,index:usize)->f32 {
    match index%TUNING_COUNT {
        0=>t.scene_guard,1=>t.local_guard,2=>t.scene_veil,3=>t.local_veil,
        4=>t.neutral_low,5=>t.neutral_high,6=>t.neutral_chroma_low,7=>t.neutral_chroma_high,
        8=>t.detail_low,9=>t.detail_high,10=>t.complexity_low,11=>t.complexity_high,
        12=>t.frost_strength,13=>t.milkiness,14=>t.interior_start,_=>t.interior_full,
    }
}
fn set_tuning_value(t:&mut MaterialTuning,index:usize,value:f32) {
    match index%TUNING_COUNT {
        0=>t.scene_guard=value,1=>t.local_guard=value,2=>t.scene_veil=value,3=>t.local_veil=value,
        4=>t.neutral_low=value,5=>t.neutral_high=value,6=>t.neutral_chroma_low=value,7=>t.neutral_chroma_high=value,
        8=>t.detail_low=value,9=>t.detail_high=value,10=>t.complexity_low=value,11=>t.complexity_high=value,
        12=>t.frost_strength=value,13=>t.milkiness=value,14=>t.interior_start=value,_=>t.interior_full=value,
    }
}
fn tuning_step(index:usize)->f32 {
    match index%TUNING_COUNT {
        4..=11|14..=15=>0.005,
        _=>0.01,
    }
}
fn parse_tuning_json(text:&str)->AppResult<MaterialTuning> {
    let body=text.trim();
    ensure(body.starts_with('{')&&body.ends_with('}'),"tuning.json must be one JSON object")?;
    let mut tuning=MaterialTuning::default();
    for item in body[1..body.len()-1].split(',') {
        let item=item.trim();
        if item.is_empty(){continue;}
        let (raw_key,raw_value)=item.split_once(':').ok_or("Invalid tuning.json entry")?;
        let key=raw_key.trim().trim_matches('"');
        let value:f32=raw_value.trim().parse()
            .map_err(|_|format!("Invalid numeric tuning value for {key}"))?;
        let index=(0..TUNING_COUNT).find(|i|tuning_name(*i)==key)
            .ok_or_else(||format!("Unknown tuning field: {key}"))?;
        set_tuning_value(&mut tuning,index,value);
    }
    Ok(tuning)
}
fn load_tuning(path:&PathBuf)->AppResult<MaterialTuning> {
    parse_tuning_json(&std::fs::read_to_string(path)?)
}
fn tuning_json(t:&MaterialTuning)->String {
    format!(
        concat!(
            "{{\n",
            "  \"scene_guard\": {:.4},\n",
            "  \"local_guard\": {:.4},\n",
            "  \"scene_veil\": {:.4},\n",
            "  \"local_veil\": {:.4},\n",
            "  \"neutral_low\": {:.4},\n",
            "  \"neutral_high\": {:.4},\n",
            "  \"neutral_chroma_low\": {:.4},\n",
            "  \"neutral_chroma_high\": {:.4},\n",
            "  \"detail_low\": {:.4},\n",
            "  \"detail_high\": {:.4},\n",
            "  \"complexity_low\": {:.4},\n",
            "  \"complexity_high\": {:.4},\n",
            "  \"frost_strength\": {:.4},\n",
            "  \"milkiness\": {:.4},\n",
            "  \"interior_start\": {:.4},\n",
            "  \"interior_full\": {:.4}\n",
            "}}\n"
        ),
        t.scene_guard,t.local_guard,t.scene_veil,t.local_veil,
        t.neutral_low,t.neutral_high,t.neutral_chroma_low,t.neutral_chroma_high,
        t.detail_low,t.detail_high,t.complexity_low,t.complexity_high,
        t.frost_strength,t.milkiness,t.interior_start,t.interior_full,
    )
}

#[cfg(test)]
mod tuning_tests {
    use super::*;

    #[test]
    fn persisted_tuning_round_trips() {
        let mut expected=MaterialTuning::default();
        expected.milkiness=0.37;
        expected.frost_strength=1.24;
        expected.local_veil=0.41;
        let parsed=parse_tuning_json(&tuning_json(&expected)).expect("saved tuning must parse");
        assert_eq!(parsed,expected);
    }

    #[test]
    fn tuning_parser_rejects_unknown_fields() {
        assert!(parse_tuning_json("{\"not_a_material_parameter\": 1.0}").is_err());
    }
}

unsafe fn resize_host_client(
    hwnd:HWND,style:WINDOW_STYLE,ex:WINDOW_EX_STYLE,width:u32,height:u32
)->AppResult<()> {
    let mut frame=RECT{left:0,top:0,right:width as i32,bottom:height as i32};
    let dpi=GetDpiForWindow(hwnd);
    ensure((96..=288).contains(&dpi),"Playground supports DPI 96..288")?;
    AdjustWindowRectExForDpi(&mut frame,style,false,ex,dpi)?;
    SetWindowPos(
        hwnd,None,0,0,frame.right-frame.left,frame.bottom-frame.top,
        SWP_NOMOVE|SWP_NOZORDER|SWP_NOACTIVATE
    )?;
    Ok(())
}

unsafe fn verify_fast_shader_parity()->AppResult<()> {
    let (device,context)=gpu::create_device(None)?;
    let initial_mask=mask_for(PreviewState::Steady)?;
    let reference=AdaptivePipeline::new_voice(device.clone(),context.clone(),W,H,&initial_mask)?;
    let mut candidate=AdaptivePipeline::new_voice(device,context,W,H,&initial_mask)?;
    let shader_dir=PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src");
    let source=AdaptivePipeline::runtime_source(&shader_dir)?;
    candidate.reload_runtime_source(&source)?;
    let cases=[
        (Fixture::WhiteText,PreviewState::Steady,false,1.0f32),
        (Fixture::Colorful,PreviewState::Listening,false,1.0),
        (Fixture::Dark,PreviewState::Optimizing,true,1.0),
        (Fixture::Green,PreviewState::Activating,false,0.20),
    ];
    let mut max_rgb=0u8;
    for (fixture,state,dark,time) in cases {
        let pixels=fixture_pixels(fixture,reference.raw.width,reference.raw.height)?;
        upload_fixture_checked(&reference,&pixels)?;
        upload_fixture_checked(&candidate,&pixels)?;
        set_state(&reference,&candidate,state)?;
        reset_pair(&reference,&candidate);
        reference.render_voice_fixture(dark,time,state.phase(),state.level(),1.0);
        candidate.render_voice_fixture(dark,time,state.phase(),state.level(),1.0);
        let a=reference.composite_fixture()?;
        let b=candidate.composite_fixture()?;
        ensure(a.len()==b.len(),"DEV/production shader parity geometry mismatch")?;
        for (pa,pb) in a.chunks_exact(4).zip(b.chunks_exact(4)) {
            ensure(pa[3]==pb[3],"DEV shader changed production alpha")?;
            for channel in 0..3 { max_rgb=max_rgb.max(pa[channel].abs_diff(pb[channel])); }
        }
    }
    ensure(max_rgb<=1,"DEV shader differs visibly from production shader")?;

    let pixels=fixture_pixels(Fixture::WhiteText,reference.raw.width,reference.raw.height)?;
    upload_fixture_checked(&candidate,&pixels)?;
    set_state(&reference,&candidate,PreviewState::Steady)?;
    candidate.reset_fixture_state();
    candidate.render_voice_fixture(false,1.0,PreviewState::Steady.phase(),0.0,1.0);
    let before=candidate.composite_fixture()?;
    let broken=source.replace("float4 adaptive_reduce_ps(","float4 adaptive_reduce_ps_missing(");
    ensure(candidate.reload_runtime_source(&broken).is_err(),"Missing reduction entry point unexpectedly compiled")?;
    candidate.reset_fixture_state();
    candidate.render_voice_fixture(false,1.0,PreviewState::Steady.phase(),0.0,1.0);
    let after=candidate.composite_fixture()?;
    ensure(before==after,"Failed two-entry reload did not preserve the atomic last-good shader pair")?;
    eprintln!("[glass-playground-fast-shader] PASS; production source with DEV-only compile hints; max RGB byte error={max_rgb}; alpha exact; second-entry failure preserves atomic last-good pair");
    Ok(())
}
pub(crate) unsafe fn check(directory:&Path)->AppResult<()> {
    std::fs::create_dir_all(directory)?;
    verify_fast_shader_parity()?;
    // Reuse existing production regressions before writing the focused evidence:
    // all adaptive background/state contracts, exact waveform palette, then the
    // strict white-text suite. All run on WARP with generated inputs only.
    gpu::adaptive_self_test(None)?;
    crate::voice_render_checks::verify(None)?;
    white_text_checks::verify(Some(directory))?;

    let mappings=[
        ("steady-material.png","steady.png"),
        ("listening-waveform.png","waveform.png"),
        ("optimizing-text.png","text.png"),
    ];
    for (source,target) in mappings {
        std::fs::copy(directory.join(source),directory.join(target))?;
    }

    eprintln!(
        "[glass-check] renderer PASS: {}; metrics.json + steady.png + waveform.png + text.png; production WARP regression, no capture/audio/network",
        directory.display()
    );
    Ok(())
}

pub(crate) unsafe fn run(apple_reference:Result<PlaygroundReferenceImage,String>)->AppResult<()> {
    CoInitializeEx(None,COINIT_APARTMENTTHREADED).ok()?;
    let _com=ComGuard;
    SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2)?;
    let factory:IDXGIFactory2=CreateDXGIFactory1()?;
    let adapter=factory.EnumAdapters1(0)?;
    let (device,context)=gpu::create_device(Some(&adapter))?;
    let initial_mask=mask_for(PreviewState::Steady)?;
    let reference=AdaptivePipeline::new_voice(device.clone(),context.clone(),W,H,&initial_mask)?;
    let mut candidate=AdaptivePipeline::new_voice(device.clone(),context.clone(),W,H,&initial_mask)?;
    ensure(reference.raw.width==candidate.raw.width&&reference.raw.height==candidate.raw.height,
        "A/B pipelines disagree on fixture geometry")?;

    let left_mask=role_mask(GlassRole::Left,PreviewState::Steady,LEFT_W,GLASS_H)?;
    let center_mask=role_mask(GlassRole::Center,PreviewState::Steady,CENTER_W,GLASS_H)?;
    let right_mask=role_mask(GlassRole::Right,PreviewState::Steady,RIGHT_W,GLASS_H)?;
    let left_reference=AdaptivePipeline::new_voice(device.clone(),context.clone(),LEFT_W,GLASS_H,&left_mask)?;
    let mut left_candidate=AdaptivePipeline::new_voice(device.clone(),context.clone(),LEFT_W,GLASS_H,&left_mask)?;
    let center_reference=AdaptivePipeline::new_voice(device.clone(),context.clone(),CENTER_W,GLASS_H,&center_mask)?;
    let mut center_candidate=AdaptivePipeline::new_voice(device.clone(),context.clone(),CENTER_W,GLASS_H,&center_mask)?;
    let right_reference=AdaptivePipeline::new_voice(device.clone(),context.clone(),RIGHT_W,GLASS_H,&right_mask)?;
    let mut right_candidate=AdaptivePipeline::new_voice(device,context,RIGHT_W,GLASS_H,&right_mask)?;

    let module=GetModuleHandleW(None)?;
    let instance=HINSTANCE(module.0);
    let host_class=w!("DoubaoGlassPlaygroundHost");
    let overlay_class=w!("DoubaoGlassPlaygroundOverlay");
    let cursor=LoadCursorW(None,IDC_ARROW).unwrap_or_default();
    ensure(RegisterClassW(&WNDCLASSW{lpfnWndProc:Some(host_proc),hInstance:instance,hCursor:cursor,lpszClassName:host_class,..Default::default()})!=0,
        "Could not register playground host window")?;
    ensure(RegisterClassW(&WNDCLASSW{lpfnWndProc:Some(overlay_proc),hInstance:instance,lpszClassName:overlay_class,..Default::default()})!=0,
        "Could not register playground overlay window")?;

    let style=WS_OVERLAPPED|WS_CAPTION|WS_SYSMENU|WS_MINIMIZEBOX;
    let ex=WS_EX_APPWINDOW;
    let host=CreateWindowExW(
        ex,host_class,w!("Liquid Glass Playground"),style,
        CW_USEDEFAULT,CW_USEDEFAULT,reference.raw.width as i32,reference.raw.height as i32,
        None,None,Some(instance),None
    )?;
    let host=Window(host);
    resize_host_client(host.0,style,ex,reference.raw.width,reference.raw.height)?;
    let host_dpi=GetDpiForWindow(host.0);
    let make_overlay=|title:PCWSTR,w:u32,h:u32|->AppResult<Window>{
        Ok(Window(CreateWindowExW(
            WS_EX_TOOLWINDOW|WS_EX_NOACTIVATE|WS_EX_NOREDIRECTIONBITMAP|WS_EX_TRANSPARENT,
            overlay_class,title,WS_POPUP,0,0,w as i32,h as i32,
            Some(host.0),None,Some(instance),None
        )?))
    };
    let overlay=make_overlay(w!("Liquid Glass Playground Material"),reference.canvas.width,reference.canvas.height)?;
    let left_overlay=make_overlay(w!("Liquid Glass Playground Material Left"),left_reference.canvas.width,left_reference.canvas.height)?;
    let center_overlay=make_overlay(w!("Liquid Glass Playground Material Center"),center_reference.canvas.width,center_reference.canvas.height)?;
    let right_overlay=make_overlay(w!("Liquid Glass Playground Material Right"),right_reference.canvas.width,right_reference.canvas.height)?;
    let _=ShowWindow(host.0,SW_SHOW);
    let _=UpdateWindow(host.0);

    let presenter=AdaptivePresenter::new(overlay.0,overlay.0,&factory,&reference)?;
    let left_presenter=AdaptivePresenter::new(left_overlay.0,left_overlay.0,&factory,&left_reference)?;
    let center_presenter=AdaptivePresenter::new(center_overlay.0,center_overlay.0,&factory,&center_reference)?;
    let right_presenter=AdaptivePresenter::new(right_overlay.0,right_overlay.0,&factory,&right_reference)?;
    let mut fixture=Fixture::AppleOptics;
    let mut state=PreviewState::Steady;
    let mut show_production=false;
    let mut show_apple=false;
    let mut dark=false;
    set_apple_scene(host.0,
        &left_reference,&left_candidate,&center_reference,&center_candidate,&right_reference,&right_candidate)?;
    set_apple_state(
        &left_reference,&left_candidate,&center_reference,&center_candidate,&right_reference,&right_candidate,state)?;
    reset_all(&[&reference,&candidate,&left_reference,&left_candidate,&center_reference,&center_candidate,&right_reference,&right_candidate]);
    resize_host_client(host.0,style,ex,SCENE_W,SCENE_H)?;
    let mut epoch=Instant::now();

    // The separate glass-playground crate decodes only the canonical WebP and
    // passes BGRA bytes here. These bytes live exclusively in PaintState/GDI and
    // are never uploaded to D3D or bound as shader resources.
    let mut apple_error:Option<String>=None;
    match apple_reference {
        Ok(reference) if reference.width>0
            && reference.height>0
            && reference.bgra.len()==(reference.width*reference.height*4) as usize => {
            PAINT.with(|state|state.borrow_mut().apple=Some(reference));
            eprintln!("[glass-playground] canonical Apple reference ready for visual-only side panel");
        }
        Ok(_)=>{
            apple_error=Some("Canonical Apple reference decoded to invalid geometry".to_string());
        }
        Err(error)=>{
            apple_error=Some(error);
        }
    }

    // Runtime source always comes from the production glass-live source directory.
    // The playground keeps no private shader copy and never feeds reference images
    // into the shader.
    let shader_dir=PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src");
    let adaptive_path=shader_dir.join("adaptive.hlsl");
    let mut shader_stamp=file_stamp(&adaptive_path);
    let mut shader_error:Option<String>=None;
    let mut reload_ms:Option<u128>=None;
    let tuning_path=PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../glass-playground/tuning.json");
    let mut tuning_stamp=file_stamp(&tuning_path);
    let mut tuning=MaterialTuning::default();
    let mut selected_tuning=13usize; // milkiness is the most common visual iteration axis.
    let mut tuning_error:Option<String>=None;
    if tuning_path.exists() {
        match load_tuning(&tuning_path).and_then(|value|{
            candidate.set_material_tuning(value)?;left_candidate.set_material_tuning(value)?;
            center_candidate.set_material_tuning(value)?;right_candidate.set_material_tuning(value)?;
            Ok(value)
        }) {
            Ok(value)=>{
                tuning=value;
                eprintln!("[glass-playground] loaded tuning from {}",tuning_path.display());
            }
            Err(error)=>{
                tuning_error=Some(error.to_string());
                eprintln!("[glass-playground] tuning load failed; keeping production defaults: {error}");
            }
        }
    }
    let compile_started=Instant::now();
    match AdaptivePipeline::runtime_source(&shader_dir).and_then(|source|{
        candidate.reload_runtime_source(&source)?;
        left_candidate.reload_runtime_source(&source)?;
        center_candidate.reload_runtime_source(&source)?;
        right_candidate.reload_runtime_source(&source)?;
        Ok(())
    }) {
        Ok(())=>{
            let elapsed=compile_started.elapsed().as_millis();
            reload_ms=Some(elapsed);
            eprintln!("[glass-playground] runtime shader ready in {elapsed}ms from {}",adaptive_path.display());
        }
        Err(error)=>{
            shader_error=Some(error.to_string());
            eprintln!("[glass-playground] initial runtime HLSL compile failed; keeping embedded last-good shader: {error}");
        }
    }
    let mut last_watch=Instant::now();
    set_title(host.0,&title(
        fixture,state,show_production,dark,shader_error.as_deref(),reload_ms,
        &tuning,selected_tuning,tuning_error.as_deref(),
        show_apple,apple_error.as_deref()
    ))?;

    let mut msg=MSG::default();
    'main: loop {
        while PeekMessageW(&mut msg,None,0,0,PM_REMOVE).as_bool() {
            if msg.message==WM_QUIT { break 'main; }
            let _=TranslateMessage(&msg);DispatchMessageW(&msg);
        }
        ensure(GetDpiForWindow(host.0)==host_dpi,
            "Display DPI changed; close and restart the playground on this display")?;
        if last_watch.elapsed()>=Duration::from_millis(25) {
            last_watch=Instant::now();
            let next=file_stamp(&adaptive_path);
            if next!=shader_stamp {
                shader_stamp=next;
                let started=Instant::now();
                match AdaptivePipeline::runtime_source(&shader_dir).and_then(|source|{
                    candidate.reload_runtime_source(&source)?;left_candidate.reload_runtime_source(&source)?;
                    center_candidate.reload_runtime_source(&source)?;right_candidate.reload_runtime_source(&source)?;Ok(())
                }) {
                    Ok(())=>{
                        let elapsed=started.elapsed().as_millis();
                        reload_ms=Some(elapsed);
                        shader_error=None;
                        reset_all(&[&reference,&candidate,&left_reference,&left_candidate,&center_reference,&center_candidate,&right_reference,&right_candidate]);
                        epoch=Instant::now();
                        eprintln!("[glass-playground] adaptive.hlsl hot-reloaded in {elapsed}ms");
                    }
                    Err(error)=>{
                        shader_error=Some(error.to_string());
                        eprintln!("[glass-playground] HLSL compile failed; rendering continues with last-good candidate: {error}");
                    }
                }
            }
            let next_tuning=file_stamp(&tuning_path);
            if next_tuning!=tuning_stamp {
                tuning_stamp=next_tuning;
                match load_tuning(&tuning_path).and_then(|value|{
                    candidate.set_material_tuning(value)?;left_candidate.set_material_tuning(value)?;
                    center_candidate.set_material_tuning(value)?;right_candidate.set_material_tuning(value)?;Ok(value)
                }) {
                    Ok(value)=>{
                        tuning=value;
                        tuning_error=None;
                        eprintln!("[glass-playground] tuning.json hot-reloaded; milkiness={:.3}; frost={:.3}; scene/local veil={:.3}/{:.3}",
                            tuning.milkiness,tuning.frost_strength,tuning.scene_veil,tuning.local_veil);
                    }
                    Err(error)=>{
                        tuning_error=Some(error.to_string());
                        eprintln!("[glass-playground] tuning reload failed; keeping last-good values: {error}");
                    }
                }
            }
        }
        let keys=KEYS.with(|keys|std::mem::take(&mut *keys.borrow_mut()));
        let mut reset=false;let mut fixture_changed=false;let mut state_changed=false;let mut capture_requested=false;
        for key in keys {
            match key as u32 {
                0x31 => {fixture=Fixture::WhiteText;fixture_changed=true;reset=true;}
                0x32 => {fixture=Fixture::White;fixture_changed=true;reset=true;}
                0x33 => {fixture=Fixture::Dark;fixture_changed=true;reset=true;}
                0x34 => {fixture=Fixture::Green;fixture_changed=true;reset=true;}
                0x35 => {fixture=Fixture::Colorful;fixture_changed=true;reset=true;}
                0x36 => {fixture=Fixture::AppleOptics;fixture_changed=true;reset=true;}
                0x20 => {state=PreviewState::Listening;state_changed=true;reset=true;}
                0x53 => {state=PreviewState::Steady;state_changed=true;reset=true;}
                0x41 => {state=PreviewState::Activating;state_changed=true;reset=true;}
                0x54 => {state=PreviewState::Optimizing;state_changed=true;reset=true;}
                0x09 => {show_production=!show_production;}
                0x43 => {capture_requested=true;}
                0x44 => {dark=!dark;reset=true;}
                0x25 => {selected_tuning=(selected_tuning+TUNING_COUNT-1)%TUNING_COUNT;}
                0x27 => {selected_tuning=(selected_tuning+1)%TUNING_COUNT;}
                0x26|0x28 => {
                    let mut next=tuning;
                    let direction=if key as u32==0x26{1.0}else{-1.0};
                    let value=tuning_value(&next,selected_tuning)+direction*tuning_step(selected_tuning);
                    set_tuning_value(&mut next,selected_tuning,value);
                    match (||->AppResult<()>{candidate.set_material_tuning(next)?;left_candidate.set_material_tuning(next)?;center_candidate.set_material_tuning(next)?;right_candidate.set_material_tuning(next)?;Ok(())})() {
                        Ok(())=>{
                            tuning=next;tuning_error=None;
                            eprintln!("[glass-playground] {}={:.4}",tuning_name(selected_tuning),tuning_value(&tuning,selected_tuning));
                        }
                        Err(error)=>{tuning_error=Some(error.to_string());}
                    }
                }
                0x50 => {
                    let next=MaterialTuning::default();
                    candidate.set_material_tuning(next)?;left_candidate.set_material_tuning(next)?;
                    center_candidate.set_material_tuning(next)?;right_candidate.set_material_tuning(next)?;
                    tuning=next;tuning_error=None;
                    eprintln!("[glass-playground] runtime tuning reset to production defaults");
                }
                0x56 => {
                    match std::fs::write(&tuning_path,tuning_json(&tuning)) {
                        Ok(())=>{
                            tuning_stamp=file_stamp(&tuning_path);
                            tuning_error=None;
                            eprintln!("[glass-playground] saved runtime tuning to {}",tuning_path.display());
                        }
                        Err(error)=>{
                            tuning_error=Some(format!("Cannot save tuning.json: {error}"));
                        }
                    }
                }
                0x52 => {
                    if apple_error.is_none() {
                        show_apple=!show_apple;
                        PAINT.with(|state|state.borrow_mut().show_apple=show_apple);
                        let (base_w,base_h)=if fixture==Fixture::AppleOptics{(SCENE_W,SCENE_H)}else{(reference.raw.width,reference.raw.height)};
                        let width=base_w*(if show_apple{2}else{1})+if show_apple{REFERENCE_GAP}else{0};
                        resize_host_client(host.0,style,ex,width,base_h)?;
                        let _=InvalidateRect(Some(host.0),None,true);
                    }
                }
                0x1B => { let _=DestroyWindow(host.0); break 'main; },
                _ => {}
            }
        }
        if fixture_changed {
            if fixture==Fixture::AppleOptics {
                set_apple_scene(host.0,&left_reference,&left_candidate,&center_reference,&center_candidate,&right_reference,&right_candidate)?;
                set_apple_state(&left_reference,&left_candidate,&center_reference,&center_candidate,&right_reference,&right_candidate,state)?;
            } else { set_fixture(host.0,&reference,&candidate,fixture)?;set_state(&reference,&candidate,state)?; }
            let (base_w,base_h)=if fixture==Fixture::AppleOptics{(SCENE_W,SCENE_H)}else{(reference.raw.width,reference.raw.height)};
            let width=base_w*(if show_apple{2}else{1})+if show_apple{REFERENCE_GAP}else{0};
            resize_host_client(host.0,style,ex,width,base_h)?;
        } else if state_changed {
            if fixture==Fixture::AppleOptics {
                set_apple_state(&left_reference,&left_candidate,&center_reference,&center_candidate,&right_reference,&right_candidate,state)?;
            } else { set_state(&reference,&candidate,state)?; }
        }
        if reset {reset_all(&[&reference,&candidate,&left_reference,&left_candidate,&center_reference,&center_candidate,&right_reference,&right_candidate]);epoch=Instant::now();}
        set_title(host.0,&title(
            fixture,state,show_production,dark,shader_error.as_deref(),reload_ms,
            &tuning,selected_tuning,tuning_error.as_deref(),
            show_apple,apple_error.as_deref()
        ))?;
        let time=epoch.elapsed().as_secs_f32();
        let level=state.level();
        if fixture==Fixture::AppleOptics {
            let _=ShowWindow(overlay.0,SW_HIDE);
            position_overlay_at(host.0,left_overlay.0,&left_reference,LEFT_X,GLASS_Y)?;
            position_overlay_at(host.0,center_overlay.0,&center_reference,CENTER_X,GLASS_Y)?;
            position_overlay_at(host.0,right_overlay.0,&right_reference,RIGHT_X,GLASS_Y)?;
            left_reference.render_voice_fixture(dark,time,PreviewState::Steady.phase(),0.0,1.0);
            left_candidate.render_voice_fixture(dark,time,PreviewState::Steady.phase(),0.0,1.0);
            center_reference.render_voice_fixture(dark,time,state.phase(),level,1.0);
            center_candidate.render_voice_fixture(dark,time,state.phase(),level,1.0);
            right_reference.render_voice_fixture(dark,time,PreviewState::Steady.phase(),0.0,1.0);
            right_candidate.render_voice_fixture(dark,time,PreviewState::Steady.phase(),0.0,1.0);
            if show_production {
                left_presenter.present(&left_reference)?;center_presenter.present(&center_reference)?;right_presenter.present(&right_reference)?;
            } else {
                left_presenter.present(&left_candidate)?;center_presenter.present(&center_candidate)?;right_presenter.present(&right_candidate)?;
            }
        } else {
            let _=ShowWindow(left_overlay.0,SW_HIDE);let _=ShowWindow(center_overlay.0,SW_HIDE);let _=ShowWindow(right_overlay.0,SW_HIDE);
            position_overlay(host.0,overlay.0,&reference)?;
            reference.render_voice_fixture(dark,time,state.phase(),level,1.0);
            candidate.render_voice_fixture(dark,time,state.phase(),level,1.0);
            if show_production { presenter.present(&reference)?; } else { presenter.present(&candidate)?; }
        }
        if capture_requested&&fixture==Fixture::AppleOptics {
            let dir=PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../glass-playground/out");
            std::fs::create_dir_all(&dir)?;let path=dir.join("apple-optics-current.png");
            if show_production {write_apple_scene_snapshot(&path,&left_reference,&center_reference,&right_reference)?;}
            else {write_apple_scene_snapshot(&path,&left_candidate,&center_candidate,&right_candidate)?;}
            eprintln!("[glass-playground] full generated scene snapshot: {}",path.display());
        }
        std::thread::sleep(Duration::from_millis(16));
    }
    let _=ShowWindow(overlay.0,SW_HIDE);let _=ShowWindow(left_overlay.0,SW_HIDE);
    let _=ShowWindow(center_overlay.0,SW_HIDE);let _=ShowWindow(right_overlay.0,SW_HIDE);
    let _=UnregisterClassW(overlay_class,Some(instance));
    let _=UnregisterClassW(host_class,Some(instance));
    Ok(())
}
