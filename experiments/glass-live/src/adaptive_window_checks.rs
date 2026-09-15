//! Native host regression on a generated, owned white paper. Never desktop
//! duplication and never user input injection. This module is cargo-test only.
use super::*;
use std::sync::{atomic::{AtomicBool,Ordering},mpsc};
use windows::Win32::Graphics::Dxgi::{CreateDXGIFactory1,IDXGIFactory2};

unsafe extern "system" fn paper_proc(h:HWND,m:u32,w:WPARAM,l:LPARAM)->LRESULT {
    match m {
        WM_MOUSEACTIVATE=>LRESULT(MA_NOACTIVATE as isize),
        WM_PAINT=>{let mut ps=PAINTSTRUCT::default();let dc=BeginPaint(h,&mut ps);
            let mut r=RECT::default();let _=GetClientRect(h,&mut r);
            let brush=CreateSolidBrush(COLORREF(0x00ffffff));FillRect(dc,&r,brush);
            let _=DeleteObject(HGDIOBJ(brush.0));let _=EndPaint(h,&ps);LRESULT(0)},
        _=>DefWindowProcW(h,m,w,l),
    }
}
struct Paper {handle:isize,stop:Arc<AtomicBool>,thread:Option<std::thread::JoinHandle<()>>}
impl Paper {
    fn new()->Self {
        let stop=Arc::new(AtomicBool::new(false));let end=stop.clone();let (tx,rx)=mpsc::sync_channel(1);
        let thread=std::thread::spawn(move||unsafe {
            let old=SetThreadDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);
            let instance=HINSTANCE(GetModuleHandleW(None).unwrap().0);
            let name=w!("AdaptiveGlassSyntheticPaper");
            assert_ne!(RegisterClassW(&WNDCLASSW{lpfnWndProc:Some(paper_proc),hInstance:instance,lpszClassName:name,..Default::default()}),0);
            let h=CreateWindowExW(WS_EX_TOOLWINDOW|WS_EX_NOACTIVATE,name,w!("Synthetic test only"),WS_POPUP,80,80,500,300,None,None,Some(instance),None).unwrap();
            let _=ShowWindow(h,SW_SHOWNOACTIVATE);let _=UpdateWindow(h);
            tx.send(h.0 as isize).unwrap();
            while !end.load(Ordering::Acquire) {
                let mut msg=MSG::default();while PeekMessageW(&mut msg,None,0,0,PM_REMOVE).as_bool(){let _=TranslateMessage(&msg);DispatchMessageW(&msg);}
                std::thread::sleep(Duration::from_millis(4));
            }
            let _=DestroyWindow(h);let _=UnregisterClassW(name,Some(instance));SetThreadDpiAwarenessContext(old);
        });
        let handle=rx.recv_timeout(Duration::from_secs(10)).expect("Synthetic paper did not initialize");
        Self{handle,stop,thread:Some(thread)}
    }
}
impl Drop for Paper {fn drop(&mut self){self.stop.store(true,Ordering::Release);if let Some(t)=self.thread.take(){let _=t.join();}}}
unsafe fn pump(){let mut m=MSG::default();while PeekMessageW(&mut m,None,0,0,PM_REMOVE).as_bool(){let _=TranslateMessage(&m);DispatchMessageW(&m);}let _=DwmFlush();}
unsafe fn rgb(x:i32,y:i32)->[u8;3]{let dc=GetDC(None);let c=GetPixel(dc,x,y).0;ReleaseDC(None,dc);assert_ne!(c,0xffffffff,"Synthetic pixel unavailable");[(c&255) as u8,((c>>8)&255) as u8,((c>>16)&255) as u8]}

#[test]
fn native_canvas_visible_input_stays_capsule_and_both_windows_move(){unsafe {
    let _serial=WINDOW_TEST_LOCK.lock().unwrap();
    let context=ThreadContext::new().unwrap();let paper=Paper::new();
    let before=GetForegroundWindow();let at=POINT{x:180,y:170};
    let (body,canvas)=create_pair(&context,at).unwrap();
    let (device,dc)=gpu::create_device(None).unwrap();
    let pipe=Pipeline::new_voice(device,dc,162,39,&vec![0u8;162*39]).unwrap();
    // ONLY this test omits capture exclusion, so GetPixel can observe generated
    // pixels. Production configure_pair always requires exclusion on both HWNDs.
    configure_pair(body.0,canvas.0,at,pipe.layout,false).unwrap();
    let input=vec![255u8;(pipe.raw.width*pipe.raw.height*4) as usize];
    crate::voice_render_checks::upload_fixture_checked(&pipe,&input).unwrap();
    let factory:IDXGIFactory2=CreateDXGIFactory1().unwrap();
    let presenter=Presenter::new(canvas.0,body.0,&factory,&pipe).unwrap();
    presenter.bind_input(&pipe);
    for n in 0..=90 {pipe.render_voice(false,n as f32/60.,Phase::Activating,0.,1.);}
    presenter.present(&pipe).unwrap();
    let _=ShowWindow(canvas.0,SW_SHOWNOACTIVATE);let _=ShowWindow(body.0,SW_SHOWNOACTIVATE);
    SetWindowPos(body.0,Some(HWND_TOPMOST),0,0,0,0,SWP_NOMOVE|SWP_NOSIZE|SWP_NOACTIVATE).unwrap();
    for _ in 0..12 {pump();std::thread::sleep(Duration::from_millis(16));}
    assert_eq!(WindowFromPoint(POINT{x:at.x+81,y:at.y+19}),body.0);
    assert_eq!(WindowFromPoint(POINT{x:at.x+81,y:at.y+46}),HWND(paper.handle as *mut _),"Shadow stole input from another thread");
    let outside=rgb(at.x+81,at.y+64);let shadow=rgb(at.x+81,at.y+44);
    assert!(outside.iter().all(|c|*c>=253),"Synthetic source obscured: {outside:?}");
    let expected=pipe.composite_fixture().unwrap();let y=pipe.padding+44;let x=pipe.padding+81;
    let target=&expected[((y*pipe.raw.width+x)*4) as usize..][..3];
    assert!(target.iter().all(|c|*c<250),"No expected white-background shadow");
    assert!((0..3).all(|k|shadow[k].abs_diff(target[k])<=5),"DWM shadow {shadow:?} differs from GPU {target:?}");
    // Inject a distinctly colored GENERATED texture to detect a body-only input
    // HWND accidentally hiding the entire lower optical visual with a blank fill.
    let colored=[86u8,183,9,255].repeat((pipe.raw.width*pipe.raw.height) as usize);
    crate::voice_render_checks::upload_fixture_checked(&pipe,&colored).unwrap();
    for n in 91..=140 {pipe.render_voice(false,n as f32/60.,Phase::Activating,0.,1.);}
    presenter.present(&pipe).unwrap();for _ in 0..12 {pump();std::thread::sleep(Duration::from_millis(16));}
    let core=rgb(at.x+81,at.y+19);
    assert!(core[1] as i32-core[0] as i32>60,"Input HWND occludes the adaptive material: {core:?}");
    let next=POINT{x:at.x+33,y:at.y+17};move_pair(body.0,canvas.0,next,pipe.layout.margin as i32).unwrap();pump();
    let mut a=RECT::default();let mut b=RECT::default();GetWindowRect(body.0,&mut a).unwrap();GetWindowRect(canvas.0,&mut b).unwrap();
    assert_eq!((a.left,a.top,a.right-a.left,a.bottom-a.top),(next.x,next.y,162,39));
    assert_eq!((a.left-b.left,a.top-b.top),(26,26));
    assert_eq!(WindowFromPoint(POINT{x:next.x+81,y:next.y+19}),body.0);
    assert_eq!(WindowFromPoint(POINT{x:next.x+81,y:next.y+46}),HWND(paper.handle as *mut _));
    assert_eq!(before,GetForegroundWindow(),"Synthetic host stole focus");
    let handles=[body.0,canvas.0];drop(presenter);drop(pipe);drop(body);drop(canvas);
    assert!(handles.iter().all(|h|!IsWindow(Some(*h)).as_bool()));
    eprintln!("[adaptive-native] PASS: real DWM body/shadow; foreign-thread margin click-through; capsule input; paired move/drop; no input injection/audio/desktop duplication");
}}
