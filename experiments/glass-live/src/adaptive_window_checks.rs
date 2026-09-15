//! Native host regression on a generated, owned white paper. Never desktop
//! duplication. Mouse routing is exercised ONLY in an explicitly enabled hosted
//! Windows CI desktop over this process's generated windows. Cargo-test only;
//! never compiled into the formal EXE self-test or run on the user's machine.
//! The adaptive workflow runs this ignored integration test explicitly, in its
//! own process, before the parallel unit suite. Other tests create windows and
//! must not compete for the same desktop/cursor while this fixture is observed.
use super::*;
use std::sync::{atomic::{AtomicBool,AtomicUsize,Ordering},mpsc};
use windows::Win32::Graphics::Dxgi::{CreateDXGIFactory1,IDXGIFactory2};
use windows::Win32::UI::Input::KeyboardAndMouse::{SendInput,INPUT as MouseInput,INPUT_0,INPUT_MOUSE,MOUSEINPUT,MOUSEEVENTF_LEFTDOWN,MOUSEEVENTF_LEFTUP};
static PAPER_CLICKS:AtomicUsize=AtomicUsize::new(0);

unsafe extern "system" fn paper_proc(h:HWND,m:u32,w:WPARAM,l:LPARAM)->LRESULT {
    match m {
        WM_LBUTTONDOWN=>{PAPER_CLICKS.fetch_add(1,Ordering::SeqCst);LRESULT(0)},
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

struct CursorRestore(POINT);
impl Drop for CursorRestore {fn drop(&mut self){unsafe{let _=SetCursorPos(self.0.x,self.0.y);}}}
unsafe fn click_fixture(p:POINT,allowed:&[HWND]) {
    let top=WindowFromPoint(p);
    assert!(allowed.contains(&top),"Unexpected window at owned test point: {top:?}");
    SetCursorPos(p.x,p.y).unwrap();
    for _ in 0..5 {pump();std::thread::sleep(Duration::from_millis(16));}
    let send=|flags|MouseInput{r#type:INPUT_MOUSE,Anonymous:INPUT_0{mi:MOUSEINPUT{dwFlags:flags,..Default::default()}}};
    assert_eq!(SendInput(&[send(MOUSEEVENTF_LEFTDOWN),send(MOUSEEVENTF_LEFTUP)],std::mem::size_of::<MouseInput>() as i32),2,"CI synthetic click injection failed");
    for _ in 0..12 {pump();std::thread::sleep(Duration::from_millis(16));}
}

#[test]
#[ignore = "requires exclusive hosted CI desktop; mandatory isolated adaptive-glass workflow step"]
fn native_canvas_visible_input_stays_capsule_and_both_windows_move(){unsafe {
    assert!(std::env::var("GITHUB_ACTIONS").as_deref()==Ok("true") && std::env::var("GLASS_CI_NATIVE_FIXTURE").as_deref()==Ok("1"),"Native desktop fixture requires explicit hosted CI authorization; it must not inject local user input");
    let _serial=WINDOW_TEST_LOCK.lock().unwrap_or_else(|p|p.into_inner());
    let mut original=POINT::default();GetCursorPos(&mut original).unwrap();let _restore=CursorRestore(original);
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
    let margin_lookup=WindowFromPoint(POINT{x:at.x+81,y:at.y+46});
    eprintln!("[adaptive-native-diagnostic] body={:?} canvas={:?} paper=0x{:X} margin_lookup={:?}; body_ex=0x{:X} canvas_ex=0x{:X}",body.0,canvas.0,paper.handle,margin_lookup,GetWindowLongPtrW(body.0,GWL_EXSTYLE),GetWindowLongPtrW(canvas.0,GWL_EXSTYLE));
    let outside=rgb(at.x+81,at.y+64);let shadow=rgb(at.x+81,at.y+44);
    assert!(outside.iter().all(|c|*c>=253),"Synthetic source obscured: {outside:?}");
    let expected=pipe.composite_fixture().unwrap();let y=pipe.padding+44;let x=pipe.padding+81;
    let target=&expected[((y*pipe.raw.width+x)*4) as usize..][..3];
    assert!(target.iter().all(|c|*c<250),"No expected white-background shadow");
    let shadow_matches=(0..3).all(|k|shadow[k].abs_diff(target[k])<=5);
    eprintln!("[adaptive-native-diagnostic] outside={outside:?} DWM_shadow={shadow:?} GPU_shadow={target:?} matches={shadow_matches}");
    // Inject a distinctly colored GENERATED texture to detect a body-only input
    // HWND accidentally hiding the entire lower optical visual with a blank fill.
    let colored=[86u8,183,9,255].repeat((pipe.raw.width*pipe.raw.height) as usize);
    crate::voice_render_checks::upload_fixture_checked(&pipe,&colored).unwrap();
    for n in 91..=140 {pipe.render_voice(false,n as f32/60.,Phase::Activating,0.,1.);}
    presenter.present(&pipe).unwrap();for _ in 0..12 {pump();std::thread::sleep(Duration::from_millis(16));}
    let core=rgb(at.x+81,at.y+19);
    let material_visible=core[1] as i32-core[0] as i32>60;
    eprintln!("[adaptive-native-diagnostic] DWM_green={core:?}; visible={material_visible}");
    PAPER_CLICKS.store(0,Ordering::SeqCst);INPUT.with(|s|s.borrow_mut().click=false);
    let owned=[body.0,canvas.0,HWND(paper.handle as *mut _)];
    click_fixture(POINT{x:at.x+81,y:at.y+46},&owned);
    let paper_received=PAPER_CLICKS.load(Ordering::SeqCst);let body_received=INPUT.with(|s|s.borrow().click);
    eprintln!("[adaptive-native-diagnostic] margin mouse events: paper={paper_received},body={body_received}");
    INPUT.with(|s|s.borrow_mut().click=false);PAPER_CLICKS.store(0,Ordering::SeqCst);
    click_fixture(POINT{x:at.x+81,y:at.y+19},&owned);
    let core_received=INPUT.with(|s|s.borrow().click);let leaked_to_paper=PAPER_CLICKS.load(Ordering::SeqCst);
    eprintln!("[adaptive-native-diagnostic] body mouse events: body={core_received},paper={leaked_to_paper}");
    let next=POINT{x:at.x+33,y:at.y+17};move_pair(body.0,canvas.0,next,pipe.layout.margin as i32).unwrap();pump();
    let mut a=RECT::default();let mut b=RECT::default();GetWindowRect(body.0,&mut a).unwrap();GetWindowRect(canvas.0,&mut b).unwrap();
    assert_eq!((a.left,a.top,a.right-a.left,a.bottom-a.top),(next.x,next.y,162,39));
    assert_eq!((a.left-b.left,a.top-b.top),(26,26));
    assert_eq!(WindowFromPoint(POINT{x:next.x+81,y:next.y+19}),body.0);
    PAPER_CLICKS.store(0,Ordering::SeqCst);INPUT.with(|s|s.borrow_mut().click=false);
    click_fixture(POINT{x:next.x+81,y:next.y+46},&owned);
    let moved_paper=PAPER_CLICKS.load(Ordering::SeqCst);let moved_body=INPUT.with(|s|s.borrow().click);
    assert!(shadow_matches && material_visible,"DWM did not display the actual complete material");
    assert!(paper_received==1&&!body_received&&core_received&&leaked_to_paper==0&&moved_paper==1&&!moved_body,"Actual CI mouse routing failed; see diagnostic event counters");
    // Verify the exact production exclusion call on both owned HWNDs only after
    // observing generated test pixels. No desktop-duplication session is opened.
    configure_pair(body.0,canvas.0,next,pipe.layout,true).unwrap();
    assert_eq!(before,GetForegroundWindow(),"Synthetic host stole focus");
    let handles=[body.0,canvas.0];drop(presenter);drop(pipe);drop(body);drop(canvas);
    assert!(handles.iter().all(|h|!IsWindow(Some(*h)).as_bool()));
    eprintln!("[adaptive-native] PASS: real DWM body/shadow; foreign-thread actual margin clicks; capsule clicks; paired move/drop; CI-owned synthetic input only; no audio/desktop duplication");
}}
