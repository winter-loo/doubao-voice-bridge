//! Dedicated Win32/D3D owner thread. Never runs in GPUI's paint callback.
//! Only this in-process window is capture-excluded. No recorder, IPC or audio.
use crate::{AppResult, ensure, desktop::{self,Capture}, gpu::{self,Pipeline,Presenter}};
use crate::voice_model::{Phase,View};
use crate::voice_overlay::{Callbacks,Event,Shared};
use std::{cell::RefCell,mem::size_of,sync::Arc,time::{Duration,Instant}};
use windows::{core::w,Win32::{
    Foundation::{HINSTANCE,HWND,LPARAM,LRESULT,POINT,RECT,WPARAM},
    Graphics::{Dwm::DwmFlush,Gdi::*},
    System::{Com::{CoInitializeEx,CoUninitialize,COINIT_APARTMENTTHREADED},LibraryLoader::GetModuleHandleW},
    UI::{HiDpi::*,Input::KeyboardAndMouse::{SetCapture,ReleaseCapture},WindowsAndMessaging::*},
}};

#[derive(Default)]
struct Input { dragging:bool,moved:bool,anchor:POINT,origin:POINT,position:Option<POINT>,click:bool,close:bool,display_changed:bool }
thread_local!{static INPUT:RefCell<Input>=RefCell::new(Input::default());}
unsafe extern "system" fn proc(hwnd:HWND,msg:u32,wp:WPARAM,lp:LPARAM)->LRESULT {
    match msg {
        WM_MOUSEACTIVATE=>LRESULT(MA_NOACTIVATE as isize),
        WM_ERASEBKGND=>LRESULT(1),
        WM_PAINT=>{let _=ValidateRect(Some(hwnd),None);LRESULT(0)},
        WM_LBUTTONDOWN=>{
            let mut p=POINT::default();let mut r=RECT::default();
            if GetCursorPos(&mut p).is_ok()&&GetWindowRect(hwnd,&mut r).is_ok(){
                INPUT.with(|s|{let mut s=s.borrow_mut();s.dragging=true;s.moved=false;s.anchor=p;s.origin=POINT{x:r.left,y:r.top};});
                SetCapture(hwnd);
            }LRESULT(0)
        },
        WM_MOUSEMOVE=>{
            let mut p=POINT::default();if GetCursorPos(&mut p).is_ok(){INPUT.with(|s|{
                let mut s=s.borrow_mut();if s.dragging{
                    let dx=p.x-s.anchor.x;let dy=p.y-s.anchor.y;if dx.abs()+dy.abs()>3{s.moved=true;}
                    if s.moved{s.position=Some(POINT{x:s.origin.x+dx,y:s.origin.y+dy});}
                }
            });}LRESULT(0)
        },
        WM_LBUTTONUP=>{INPUT.with(|s|{let mut s=s.borrow_mut();if s.dragging&&!s.moved{s.click=true;}s.dragging=false;});let _=ReleaseCapture();LRESULT(0)},
        WM_CAPTURECHANGED=>{INPUT.with(|s|s.borrow_mut().dragging=false);LRESULT(0)},
        WM_CLOSE=>{INPUT.with(|s|s.borrow_mut().close=true);LRESULT(0)},
        WM_DISPLAYCHANGE|WM_DPICHANGED=>{INPUT.with(|s|s.borrow_mut().display_changed=true);LRESULT(0)},
        WM_ENDSESSION if wp.0!=0=>{INPUT.with(|s|s.borrow_mut().close=true);LRESULT(0)},
        // Preview save/theme/right-click semantics are intentionally absent.
        WM_RBUTTONDOWN|WM_MBUTTONDOWN=>LRESULT(0),
        _=>DefWindowProcW(hwnd,msg,wp,lp),
    }
}
struct Window(HWND);
impl Drop for Window {fn drop(&mut self){unsafe{let _=ShowWindow(self.0,SW_HIDE);if IsWindow(Some(self.0)).as_bool(){let _=DestroyWindow(self.0);}}}}
struct ThreadContext {old:DPI_AWARENESS_CONTEXT,instance:HINSTANCE}
impl ThreadContext {
    unsafe fn new()->AppResult<Self>{
        CoInitializeEx(None,COINIT_APARTMENTTHREADED).ok()?;
        let old=SetThreadDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);
        if old.0.is_null(){CoUninitialize();return Err("Cannot enter physical-pixel context".into());}
        let module=match GetModuleHandleW(None){Ok(m)=>m,Err(e)=>{SetThreadDpiAwarenessContext(old);CoUninitialize();return Err(e.into());}};
        let instance=HINSTANCE(module.0);
        let cursor=LoadCursorW(None,IDC_HAND).unwrap_or_default();
        if RegisterClassW(&WNDCLASSW{lpfnWndProc:Some(proc),hInstance:instance,hCursor:cursor,lpszClassName:w!("DoubaoVoiceLiquidOverlay"),..Default::default()})==0{
            SetThreadDpiAwarenessContext(old);CoUninitialize();return Err("Could not register production glass window".into());
        }
        Ok(Self{old,instance})
    }
}
impl Drop for ThreadContext {fn drop(&mut self){unsafe{
    let _=UnregisterClassW(w!("DoubaoVoiceLiquidOverlay"),Some(self.instance));SetThreadDpiAwarenessContext(self.old);CoUninitialize();
}}}

// Field order deliberately releases composition/capture/GPU BEFORE the HWND.
struct Session {
    presenter:Presenter,capture:Capture,pipe:Pipeline,window:Window,
    generation:u64,position:POINT,work:RECT,scale:f32,start:Instant,
    phase:Phase,shown:bool,have_frame:bool,
}
impl Session {
    unsafe fn new(view:View,context:&ThreadContext,previous:Option<POINT>,shared:&Shared)->AppResult<Self>{
        ensure(shared.allows(view.generation),"Session superseded before graphics initialization")?;
        let (factory,adapter,output,_desc,monitor)=desktop::choose_output()?;
        INPUT.with(|s|*s.borrow_mut()=Input::default());
        let work=monitor.rcWork;
        let hwnd=CreateWindowExW(WS_EX_TOPMOST|WS_EX_TOOLWINDOW|WS_EX_NOACTIVATE|WS_EX_NOREDIRECTIONBITMAP,
            w!("DoubaoVoiceLiquidOverlay"),w!("Doubao Voice Liquid Glass"),WS_POPUP,
            (work.left+work.right)/2,(work.top+work.bottom)/2,1,1,None,None,Some(context.instance),None)?;
        let window=Window(hwnd);
        let dpi=GetDpiForWindow(hwnd);ensure((96..=288).contains(&dpi),"Unsupported overlay DPI; use solid fallback")?;
        let scale=dpi as f32/96.;let width=(108.*scale).round() as u32;let height=(26.*scale).round() as u32;
        let mask=content_mask(width,height,scale,view.phase)?;
        let (device,device_context)=gpu::create_device(Some(&adapter))?;
        let pipe=Pipeline::new_voice(device,device_context,width,height,&mask)?;
        let pad=pipe.padding as i32;
        ensure(work.right-work.left>width as i32+pad*2&&work.bottom-work.top>height as i32+pad*2,"Work area too small")?;
        let proposed=previous.unwrap_or(POINT{x:(work.left+work.right-width as i32)/2,y:work.bottom-height as i32-pad-20});
        let position=clamp_position(proposed,work,width,height,pad);
        SetWindowPos(hwnd,Some(HWND_TOPMOST),position.x,position.y,width as i32,height as i32,SWP_NOACTIVATE)?;
        let region=CreateRoundRectRgn(0,0,width as i32+1,height as i32+1,height as i32,height as i32);
        ensure(!region.0.is_null(),"Cannot allocate glass hit region")?;
        if SetWindowRgn(hwnd,Some(region),false)==0{let _=DeleteObject(HGDIOBJ(region.0));return Err("Cannot set glass hit region".into());}
        SetWindowDisplayAffinity(hwnd,WDA_EXCLUDEFROMCAPTURE)?;
        let mut affinity=0;GetWindowDisplayAffinity(hwnd,&mut affinity)?;
        ensure(affinity==WDA_EXCLUDEFROMCAPTURE.0,"Capture exclusion unavailable; refuse recursive sampling")?;
        let presenter=Presenter::new(hwnd,&factory,&pipe.device,width,height)?;
        presenter.bind_input(&pipe);
        // Only after explicit consent, current generation checks and exclusion.
        ensure(shared.allows(view.generation),"Session superseded before desktop capture")?;
        let capture=Capture{duplication:output.DuplicateOutput(&pipe.device)?,cache:None,rect:_desc.DesktopCoordinates,frames:0};
        Ok(Self{presenter,capture,pipe,window,generation:view.generation,position,work,scale,start:Instant::now(),phase:view.phase,shown:false,have_frame:false})
    }
    unsafe fn frame(&mut self,view:View,shared:&Shared,callbacks:Callbacks)->AppResult<bool>{
        let mut msg=MSG::default();
        for _ in 0..64{if !PeekMessageW(&mut msg,None,0,0,PM_REMOVE).as_bool(){break;}
            if msg.message==WM_QUIT{return Err("Window message loop ended".into());}
            let _=TranslateMessage(&msg);DispatchMessageW(&msg);
        }
        let (position,click,close,changed)=INPUT.with(|s|{let mut s=s.borrow_mut();let r=(s.position.take(),s.click,s.close,s.display_changed);s.click=false;r});
        if changed{return Err("Display/DPI changed; capture released, retry next voice session".into());}
        if (click||close)&&view.phase.can_finish(){(callbacks.event)(Event::FinishRequested(view.generation));}
        if close{return Err("Glass window closed; retain voice feedback through fallback".into());}
        if !shared.allows(self.generation){return Ok(false);}
        let mut moved=false;
        if let Some(p)=position {
            let p=clamp_position(p,self.work,self.pipe.output.width,self.pipe.output.height,self.pipe.padding as i32);
            moved=p.x!=self.position.x||p.y!=self.position.y;
            if moved{SetWindowPos(self.window.0,None,p.x,p.y,0,0,SWP_NOSIZE|SWP_NOZORDER|SWP_NOACTIVATE)?;self.position=p;}
        }
        let fresh=self.capture.acquire(&self.pipe)?;
        if fresh{self.have_frame=true;}
        if !self.have_frame {
            ensure(self.start.elapsed()<Duration::from_secs(2),"No valid desktop frame within two seconds")?;
            return Ok(true);
        }
        if fresh||moved{self.capture.crop(&self.pipe,self.position)?;self.pipe.prepare();}
        if view.phase!=self.phase{
            let mask=content_mask(self.pipe.output.width,self.pipe.output.height,self.scale,view.phase)?;
            self.pipe.set_voice_mask(&mask)?;self.phase=view.phase;
        }
        let level=(callbacks.level)();
        self.pipe.render_voice(view.dark,self.start.elapsed().as_secs_f32(),view.phase,level,view.opacity);
        // A completion/new-session arriving during a GPU pass must not reshow old content.
        if !shared.allows(self.generation){return Ok(false);}
        self.presenter.present(&self.pipe)?;
        if !self.shown {
            let _=ShowWindow(self.window.0,SW_SHOWNOACTIVATE);
            SetWindowPos(self.window.0,Some(HWND_TOPMOST),0,0,0,0,SWP_NOMOVE|SWP_NOSIZE|SWP_NOACTIVATE)?;
            self.shown=true;shared.active(true,callbacks);
            eprintln!("[voice-glass] LIVE generation={}; material=LENS_TRANSMISSION_1; in-process; actual voice state; no microphone/save API",self.generation);
        }
        Ok(true)
    }
}
impl Drop for Session {fn drop(&mut self){unsafe{let _=ShowWindow(self.window.0,SW_HIDE);}
    eprintln!("[voice-glass] session released; generation={}; desktop_frames={}; no screen images saved",self.generation,self.capture.frames);
}}
fn clamp_position(p:POINT,work:RECT,w:u32,h:u32,pad:i32)->POINT {
    POINT{x:p.x.clamp(work.left+pad,work.right-w as i32-pad),y:p.y.clamp(work.top+pad,work.bottom-h as i32-pad)}
}
pub(crate) unsafe fn content_mask(w:u32,h:u32,scale:f32,phase:Phase)->AppResult<Vec<u8>> {
    if phase==Phase::Listening||phase==Phase::Hidden{return Ok(vec![0;(w*h) as usize]);}
    desktop::text_mask_for_label(w,h,scale,true,phase.label(),phase==Phase::Optimizing)
}
unsafe fn owned_window(hwnd:HWND)->bool{
    let mut pid=0;GetWindowThreadProcessId(hwnd,Some(&mut pid));
    pid==std::process::id() && IsWindow(Some(hwnd)).as_bool()
}
unsafe fn fallback(hwnd:HWND,show:bool){
    if !owned_window(hwnd){return;}
    if IsWindowVisible(hwnd).as_bool()!=show{let _=ShowWindow(hwnd,if show{SW_SHOWNOACTIVATE}else{SW_HIDE});}
}
pub(crate) fn restore_after_panic(shared:&Shared,fallback_hwnd:isize){unsafe{
    let (view,stop,_)=shared.view();fallback(HWND(fallback_hwnd as *mut _),!stop&&view.visible()&&!matches!(view.phase,Phase::Completed|Phase::Failed));
}}

pub(crate) fn run(shared:Arc<Shared>,fallback_hwnd:isize,callbacks:Callbacks){unsafe{
    let fallback_hwnd=HWND(fallback_hwnd as *mut _);
    let mut context:Option<ThreadContext>=None;
    let mut session:Option<Session>=None;
    let mut failed_generation=None;
    let mut previous=None;
    let mut managed=false;
    loop {
        let tick=Instant::now();let (view,stop,revision)=shared.view();
        if stop{break;}
        if !view.enabled {
            session.take();shared.active(false,callbacks);
            if managed{fallback(fallback_hwnd,view.visible()&&!matches!(view.phase,Phase::Completed|Phase::Failed));managed=false;}
            failed_generation=None;
            shared.wait(revision);continue;
        }
        managed=true;
        if !owned_window(fallback_hwnd){break;}
        if !view.visible(){
            session.take();shared.active(false,callbacks);fallback(fallback_hwnd,false);failed_generation=None;
            shared.wait(revision);continue;
        }
        if session.as_ref().is_some_and(|s|s.generation!=view.generation){session.take();shared.active(false,callbacks);}
        if failed_generation==Some(view.generation){
            fallback(fallback_hwnd,!matches!(view.phase,Phase::Completed|Phase::Failed));
            // Terminal expiry uses monotonic time even without a graphics device.
            std::thread::sleep(Duration::from_millis(16));continue;
        }
        if session.is_none(){
            // Do not sample the application's old fallback into its new glass.
            fallback(fallback_hwnd,false);let _=DwmFlush();
            let result=(||->AppResult<Session>{
                if context.is_none(){context=Some(ThreadContext::new()?);}
                Session::new(view,context.as_ref().unwrap(),previous,&shared)
            })();
            match result{
                Ok(s)=>session=Some(s),
                Err(e)=>{
                    if shared.allows(view.generation){failed_generation=Some(view.generation);(callbacks.event)(Event::Unavailable(e.to_string()));}
                    continue;
                }
            }
        }
        let result=session.as_mut().unwrap().frame(view,&shared,callbacks);
        previous=session.as_ref().map(|s|s.position);
        match result{
            Ok(true)=>{},
            Ok(false)=>{session.take();shared.active(false,callbacks);},
            Err(e)=>{session.take();shared.active(false,callbacks);failed_generation=Some(view.generation);
                (callbacks.event)(Event::Unavailable(e.to_string()));}
        }
        if let Some(wait)=Duration::from_millis(16).checked_sub(tick.elapsed()){std::thread::sleep(wait);}
    }
    session.take();shared.active(false,callbacks);
    if managed{fallback(fallback_hwnd,false);}
    drop(context);
}}

#[cfg(test)] mod tests {
    use super::*;
    #[test] fn fixed_geometry_stays_inside_roi_and_only_recording_can_finish(){
        let work=RECT{left:0,top:0,right:1920,bottom:1040};let p=clamp_position(POINT{x:-999,y:99999},work,162,39,26);
        assert_eq!((p.x,p.y),(26,975));
        assert!(Phase::Listening.can_finish()&&Phase::Activating.can_finish());
        assert!(!Phase::Optimizing.can_finish()&&!Phase::Completed.can_finish()&&!Phase::Failed.can_finish());
    }
    #[test] fn production_window_messages_never_expose_preview_controls(){unsafe{
        let _context=ThreadContext::new().unwrap();
        let h=CreateWindowExW(WS_EX_TOOLWINDOW,w!("DoubaoVoiceLiquidOverlay"),w!("hidden test"),WS_POPUP,0,0,1,1,None,None,Some(_context.instance),None).unwrap();
        let _window=Window(h);INPUT.with(|s|*s.borrow_mut()=Input::default());
        assert_eq!(proc(h,WM_MOUSEACTIVATE,WPARAM(0),LPARAM(0)).0,MA_NOACTIVATE as isize);
        proc(h,WM_RBUTTONDOWN,WPARAM(0),LPARAM(0));proc(h,WM_MBUTTONDOWN,WPARAM(0),LPARAM(0));
        proc(h,0x8031,WPARAM(1),LPARAM(0));
        assert!(IsWindow(Some(h)).as_bool());assert!(!INPUT.with(|s|s.borrow().click||s.borrow().close));
        // No ShowWindow, desktop duplication, audio, system input injection or hotkeys.
    }}
}
