//! Application adapter for the shared optical engine. No second voice client.
use doubao_glass_live::{voice_model::Phase, voice_overlay::{Callbacks,Controller,Event}};
use std::{io::Write, sync::{OnceLock,atomic::{AtomicBool,AtomicIsize,Ordering}}};
use crate::OverlayPhase;
static ENGINE:OnceLock<Controller>=OnceLock::new();
static REQUESTED:AtomicBool=AtomicBool::new(false);
static FALLBACK:AtomicIsize=AtomicIsize::new(0);

fn system_dark()->bool {
    use windows::{core::w,Win32::System::Registry::{RegGetValueW,HKEY_CURRENT_USER,RRF_RT_REG_DWORD}};
    let mut value=1u32;let mut size=4u32;
    unsafe { RegGetValueW(HKEY_CURRENT_USER,w!("Software\\Microsoft\\Windows\\CurrentVersion\\Themes\\Personalize"),w!("AppsUseLightTheme"),RRF_RT_REG_DWORD,None,Some((&mut value as *mut u32).cast()),Some(&mut size)).is_ok() && value==0 }
}
pub fn refresh_theme(){if let Some(e)=ENGINE.get(){e.set_dark(system_dark());}}
pub fn initialize(fallback:isize) {
    FALLBACK.store(fallback,Ordering::Release);
    let settings=crate::client_settings::ClientSettings::load().unwrap_or_default();
    let requested=if std::env::args().any(|a|a=="--no-liquid-glass"){false}
        else{settings.liquid_glass||std::env::args().any(|a|a=="--liquid-glass")};
    REQUESTED.store(requested,Ordering::Release);
    match Controller::new(fallback,requested,system_dark(),Callbacks{event:on_event,level:crate::voice_activity}) {
        Ok(engine)=>{let _=ENGINE.set(engine);log("initialized; same-process optical engine; idle until authorized visible voice session");},
        Err(e)=>log(&format!("initialization failed; solid fallback: {e}")),
    }
}
pub fn enabled()->bool{REQUESTED.load(Ordering::Acquire)}
pub fn owns_visibility()->bool{ENGINE.get().is_some_and(Controller::owns_visibility)}
pub fn presenting()->bool{ENGINE.get().is_some_and(Controller::presenting)}
pub fn configure(enabled:bool) {
    REQUESTED.store(enabled,Ordering::Release);
    if let Some(e)=ENGINE.get(){e.set_enabled(enabled);}
    log(if enabled{"user enabled liquid glass; local GPU desktop sampling while visible; overlay excluded from system capture"}
        else{"user disabled liquid glass; renderer relinquishes desktop capture"});
}
pub fn phase(phase:OverlayPhase,generation:u64){if let Some(e)=ENGINE.get(){
    if phase!=OverlayPhase::Hidden{e.set_dark(system_dark());}
    e.phase(generation,match phase{
        OverlayPhase::Hidden=>Phase::Hidden,OverlayPhase::Activating=>Phase::Activating,
        OverlayPhase::Listening=>Phase::Listening,OverlayPhase::Optimizing=>Phase::Optimizing,
    });
}}
pub fn finished(generation:u64){if let Some(e)=ENGINE.get(){e.finished(generation);}}
pub fn failed(generation:u64){if let Some(e)=ENGINE.get(){e.failed(generation);}}
pub fn shutdown(){if let Some(e)=ENGINE.get(){e.shutdown();}}
fn on_event(event:Event){match event{
    Event::Presenting(active)=>log(if active{"selected=Liquid; material=LENS_ADAPTIVE_2; voice-state content"}else{"optical surface released"}),
    Event::Unavailable(error)=>log(&format!("selected=Solid; voice session preserved; {error}")),
    Event::FinishRequested(generation)=>{
        if generation==crate::overlay_generation() && matches!(crate::overlay_phase(),OverlayPhase::Activating|OverlayPhase::Listening){
            let hwnd=windows::Win32::Foundation::HWND(FALLBACK.load(Ordering::Acquire) as *mut _);
            crate::platform::finish_input_hwnd(hwnd);
        }
    },
}}
fn log(message:&str){
    if let Ok(mut f)=std::fs::OpenOptions::new().create(true).append(true).open(std::env::temp_dir().join("doubao-voice-glass.log")){
        let _=writeln!(f,"[voice-glass] {message}");
    }
}
/// Runs before single-instance, GPUI, shell or voice initialization. Real optical
/// shader/production-content checks; generated fixture input only, never desktop.
pub fn self_test_requested()->bool{
    if !std::env::args().any(|a|a=="--liquid-glass-self-test"){return false;}
    match doubao_glass_live::voice_self_test(){
        Ok(())=>{println!("[voice-glass-self-test] PASS; actual in-process production renderer; no desktop/audio/network");},
        Err(error)=>{eprintln!("[voice-glass-self-test] FAIL: {error}");std::process::exit(1);},
    }
    true
}
