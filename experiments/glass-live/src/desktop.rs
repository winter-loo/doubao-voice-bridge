//! Original standalone host: one small DirectComposition window, no HostBackdrop.
//! Desktop frames stay GPU-resident. Only an explicit middle click performs a
//! bounded local readback when a snapshot directory was supplied by the caller.
use crate::{ensure, gpu::{self, Pipeline, Presenter}, AppResult, Options};
use std::{cell::RefCell, mem::size_of, time::{Duration, Instant}};
use windows::{core::{w, Interface}, Win32::{
    Foundation::{COLORREF, HINSTANCE, HWND, LPARAM, LRESULT, POINT, RECT, WPARAM},
    Graphics::{Direct3D11::*, Dxgi::{Common::*, *}, Gdi::*},
    System::{Com::{CoInitializeEx, CoUninitialize, COINIT_APARTMENTTHREADED}, LibraryLoader::GetModuleHandleW},
    UI::{HiDpi::*, Input::KeyboardAndMouse::{SetCapture, ReleaseCapture}, WindowsAndMessaging::*},
}};

#[derive(Default)]
struct Input {
    dragging: bool, moved: bool, anchor: POINT, original: POINT,
    new_position: Option<POINT>, toggle: bool, snapshot: bool, display_changed: bool,
}
thread_local! { static INPUT: RefCell<Input> = RefCell::new(Input::default()); }
unsafe extern "system" fn window_proc(hwnd: HWND, message: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    match message {
        WM_MOUSEACTIVATE => LRESULT(MA_NOACTIVATE as isize),
        WM_ERASEBKGND => LRESULT(1),
        WM_PAINT => { let _ = ValidateRect(Some(hwnd), None); LRESULT(0) }
        WM_LBUTTONDOWN => {
            let mut cursor = POINT::default(); let mut rect = RECT::default();
            if GetCursorPos(&mut cursor).is_ok() && GetWindowRect(hwnd, &mut rect).is_ok() {
                INPUT.with(|s| { let mut s=s.borrow_mut(); s.dragging=true; s.moved=false;
                    s.anchor=cursor; s.original=POINT{x:rect.left,y:rect.top}; });
                SetCapture(hwnd);
            }
            LRESULT(0)
        }
        WM_MOUSEMOVE => {
            let mut p=POINT::default();
            if GetCursorPos(&mut p).is_ok() { INPUT.with(|s| {
                let mut s=s.borrow_mut(); if s.dragging {
                    let dx=p.x-s.anchor.x; let dy=p.y-s.anchor.y;
                    if dx.abs()+dy.abs()>3 { s.moved=true; }
                    if s.moved { s.new_position=Some(POINT{x:s.original.x+dx,y:s.original.y+dy}); }
                }
            }); }
            LRESULT(0)
        }
        WM_LBUTTONUP => {
            INPUT.with(|s| {let mut s=s.borrow_mut(); if s.dragging&&!s.moved {s.toggle=true;} s.dragging=false;});
            let _=ReleaseCapture(); LRESULT(0)
        }
        WM_CAPTURECHANGED => {INPUT.with(|s|s.borrow_mut().dragging=false);LRESULT(0)}
        WM_MBUTTONDOWN => {INPUT.with(|s|s.borrow_mut().snapshot=true);LRESULT(0)}
        WM_RBUTTONDOWN | WM_CLOSE => {let _=DestroyWindow(hwnd);LRESULT(0)}
        WM_DISPLAYCHANGE | WM_DPICHANGED => {INPUT.with(|s|s.borrow_mut().display_changed=true);LRESULT(0)}
        WM_DESTROY => {PostQuitMessage(0);LRESULT(0)}
        _ => DefWindowProcW(hwnd,message,wp,lp),
    }
}
struct Window(HWND);
impl Drop for Window {
    fn drop(&mut self) {unsafe {if IsWindow(Some(self.0)).as_bool() {let _=DestroyWindow(self.0);}}}
}
struct Apartment;
impl Drop for Apartment {fn drop(&mut self){unsafe{CoUninitialize()}}}

struct Capture {
    duplication: IDXGIOutputDuplication,
    cache: Option<ID3D11Texture2D>,
    pub rect: RECT,
    pub frames: u64,
}
impl Capture {
    unsafe fn acquire(&mut self, pipe: &Pipeline) -> AppResult<bool> {
        let mut info=DXGI_OUTDUPL_FRAME_INFO::default(); let mut resource=None;
        match self.duplication.AcquireNextFrame(0,&mut info,&mut resource) {
            Err(e) if e.code()==DXGI_ERROR_WAIT_TIMEOUT => return Ok(false),
            Err(e) => return Err(format!("Desktop capture lost/unavailable ({e}); preview stops instead of displaying a stale surface").into()),
            Ok(()) => {}
        }
        // ReleaseFrame on EVERY success path, including subsequent failures.
        let result=(|| -> AppResult<()> {
            let resource=resource.ok_or("Duplication returned no frame")?;
            let frame:ID3D11Texture2D=resource.cast()?;
            let mut desc=D3D11_TEXTURE2D_DESC::default(); frame.GetDesc(&mut desc);
            ensure(desc.Format==DXGI_FORMAT_B8G8R8A8_UNORM && desc.Width==(self.rect.right-self.rect.left) as u32
                && desc.Height==(self.rect.bottom-self.rect.top) as u32,"Unsupported captured size/format change")?;
            if self.cache.is_none() {
                desc.Usage=D3D11_USAGE_DEFAULT;desc.BindFlags=0;desc.CPUAccessFlags=0;desc.MiscFlags=0;
                let mut cache=None;pipe.device.CreateTexture2D(&desc,None,Some(&mut cache))?;
                self.cache=Some(cache.ok_or("No desktop cache")?);
            }
            pipe.context.CopyResource(self.cache.as_ref().unwrap(),&frame);
            self.frames+=1;Ok(())
        })();
        let released=self.duplication.ReleaseFrame();result?;released?;Ok(true)
    }
    unsafe fn crop(&self, pipe:&Pipeline, position:POINT) -> AppResult<()> {
        let x=position.x-self.rect.left-pipe.padding as i32;
        let y=position.y-self.rect.top-pipe.padding as i32;
        ensure(x>=0&&y>=0&&x+pipe.raw.width as i32<=self.rect.right-self.rect.left
            &&y+pipe.raw.height as i32<=self.rect.bottom-self.rect.top,"Capture ROI lies outside the output")?;
        let cache=self.cache.as_ref().ok_or("No valid captured frame")?;
        let source=D3D11_BOX{left:x as u32,top:y as u32,front:0,right:x as u32+pipe.raw.width,bottom:y as u32+pipe.raw.height,back:1};
        pipe.context.CopySubresourceRegion(&pipe.raw.texture,0,0,0,0,cache,0,Some(&source));Ok(())
    }
}

unsafe fn choose_output() -> AppResult<(IDXGIFactory2,IDXGIAdapter1,IDXGIOutput1,DXGI_OUTPUT_DESC,MONITORINFO)> {
    ensure(GetSystemMetrics(SM_CMONITORS)==1,"This first preview supports one SDR, unrotated display. No display settings were changed")?;
    let factory:IDXGIFactory2=CreateDXGIFactory1()?;
    let mut index=0;
    loop {
        let adapter=match factory.EnumAdapters1(index){Ok(a)=>a,Err(e)if e.code()==DXGI_ERROR_NOT_FOUND=>break,Err(e)=>return Err(e.into())};
        let mut oi=0;
        loop {
            let output=match adapter.EnumOutputs(oi){Ok(o)=>o,Err(e)if e.code()==DXGI_ERROR_NOT_FOUND=>break,Err(e)=>return Err(e.into())};
            let desc=output.GetDesc()?;
            let mut monitor=MONITORINFO{cbSize:size_of::<MONITORINFO>()as u32,..Default::default()};
            if desc.AttachedToDesktop.as_bool() && GetMonitorInfoW(desc.Monitor,&mut monitor).as_bool() && monitor.dwFlags&1!=0 {
                ensure(desc.Rotation==DXGI_MODE_ROTATION_IDENTITY,"Rotated displays are not supported by this preview")?;
                let output6:IDXGIOutput6=output.cast()?;let details=output6.GetDesc1()?;
                ensure(details.ColorSpace==DXGI_COLOR_SPACE_RGB_FULL_G22_NONE_P709,
                    "HDR/advanced-color output detected; this milestone is SDR-only and will not silently tone-map or change settings")?;
                let ad=adapter.GetDesc1()?;let n=ad.Description.iter().position(|&v|v==0).unwrap_or(ad.Description.len());
                eprintln!("[glass-live] adapter={}; output={}x{}; SDR; desktop pixels remain on GPU unless a snapshot is requested",
                    String::from_utf16_lossy(&ad.Description[..n]),desc.DesktopCoordinates.right-desc.DesktopCoordinates.left,
                    desc.DesktopCoordinates.bottom-desc.DesktopCoordinates.top);
                return Ok((factory,adapter,output.cast()?,desc,monitor));
            }
            oi+=1;
        }
        index+=1;
    }
    Err("Primary active output not found".into())
}

/// GDI is used ONCE to rasterize our own text coverage, never for desktop capture.
unsafe fn text_mask(width:u32,height:u32,scale:f32,compact:bool)->AppResult<Vec<u8>> {
    let dc=CreateCompatibleDC(None);ensure(!dc.0.is_null(),"Text mask DC creation failed")?;
    let result=(||->AppResult<Vec<u8>>{
        let mut info=BITMAPINFO::default();info.bmiHeader=BITMAPINFOHEADER{biSize:size_of::<BITMAPINFOHEADER>()as u32,
            biWidth:width as i32,biHeight:-(height as i32),biPlanes:1,biBitCount:32,biCompression:BI_RGB.0,..Default::default()};
        let mut bits=std::ptr::null_mut();let bitmap=CreateDIBSection(Some(dc),&info,DIB_RGB_COLORS,&mut bits,None,0)?;
        let old=SelectObject(dc,HGDIOBJ(bitmap.0));
        let font=CreateFontW(-((if compact {11.}else{14.})*scale).round()as i32,0,0,0,500,0,0,0,
            DEFAULT_CHARSET.0 as u32,OUT_DEFAULT_PRECIS.0 as u32,CLIP_DEFAULT_PRECIS.0 as u32,
            ANTIALIASED_QUALITY.0 as u32,(DEFAULT_PITCH.0|FF_DONTCARE.0)as u32,w!("Microsoft YaHei UI"));
        if font.0.is_null(){SelectObject(dc,old);let _=DeleteObject(HGDIOBJ(bitmap.0));return Err("Text font creation failed".into());}
        let old_font=SelectObject(dc,HGDIOBJ(font.0));
        std::ptr::write_bytes(bits,0,(width*height*4)as usize);
        SetBkMode(dc,TRANSPARENT);SetTextColor(dc,COLORREF(0x00ffffff));
        let mut rect=RECT{left:(height as f32*.92)as i32,top:0,right:width as i32-(height as f32*.25)as i32,bottom:height as i32};
        let mut text:Vec<u16>="优化识别中".encode_utf16().collect();
        let drawn=DrawTextW(dc,&mut text,&mut rect,DT_CENTER|DT_VCENTER|DT_SINGLELINE);
        let pixels=std::slice::from_raw_parts(bits as *const u8,(width*height*4)as usize);
        let mask=pixels.chunks_exact(4).map(|p|p[0].max(p[1]).max(p[2])).collect::<Vec<_>>();
        SelectObject(dc,old_font);let _=DeleteObject(HGDIOBJ(font.0));SelectObject(dc,old);let _=DeleteObject(HGDIOBJ(bitmap.0));
        ensure(drawn>0&&mask.iter().any(|&v|v>0),"Foreground text mask is empty")?;Ok(mask)
    })();
    let _=DeleteDC(dc);result
}

pub unsafe fn run(options:Options)->AppResult<()> {
    ensure(options.allow_capture,"Capture was not authorized")?;
    CoInitializeEx(None,COINIT_APARTMENTTHREADED).ok()?;let _apartment=Apartment;
    SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2)?;
    let (factory,adapter,output,desc,monitor)=choose_output()?;
    let module=GetModuleHandleW(None)?;
    let class=w!("DoubaoGlassLivePreviewV1");
    let atom=RegisterClassW(&WNDCLASSW{lpfnWndProc:Some(window_proc),hInstance:HINSTANCE(module.0),
        hCursor:LoadCursorW(None,IDC_HAND)?,lpszClassName:class,..Default::default()});
    ensure(atom!=0,"Preview class registration failed")?;
    let work=monitor.rcWork;
    let hwnd=CreateWindowExW(WS_EX_TOPMOST|WS_EX_TOOLWINDOW|WS_EX_NOACTIVATE|WS_EX_NOREDIRECTIONBITMAP,
        class,w!("Doubao Custom Glass Preview - no microphone"),WS_POPUP,(work.left+work.right)/2,(work.top+work.bottom)/2,
        1,1,None,None,Some(HINSTANCE(module.0)),None)?;
    let window=Window(hwnd);
    let dpi=GetDpiForWindow(hwnd);ensure((96..=288).contains(&dpi),"Preview supports DPI 96..288")?;
    let scale=dpi as f32/96.;
    let width=((if options.compact{108.}else{180.})*scale).round()as u32;
    let height=((if options.compact{26.}else{40.})*scale).round()as u32;
    let mask=text_mask(width,height,scale,options.compact)?;
    let (device,context)=gpu::create_device(Some(&adapter))?;
    let pipe=Pipeline::new(device,context,width,height,&mask)?;
    let pad=pipe.padding as i32;
    let mut position=POINT{x:(work.left+work.right-width as i32)/2,y:work.bottom-height as i32-pad-20};
    ensure(work.right-work.left>width as i32+pad*2&&work.bottom-work.top>height as i32+pad*2,"Work area is too small")?;
    SetWindowPos(hwnd,Some(HWND_TOPMOST),position.x,position.y,width as i32,height as i32,SWP_NOACTIVATE)?;
    let region=CreateRoundRectRgn(0,0,width as i32+1,height as i32+1,height as i32,height as i32);
    ensure(!region.0.is_null(),"Capsule region creation failed")?;
    if SetWindowRgn(hwnd,Some(region),false)==0 {let _=DeleteObject(HGDIOBJ(region.0));return Err("Capsule window region failed".into());}
    // Strictly scoped to this temporary HWND, set before SHOW and capture start.
    SetWindowDisplayAffinity(hwnd,WDA_EXCLUDEFROMCAPTURE)?;
    let mut affinity=0u32;GetWindowDisplayAffinity(hwnd,&mut affinity)?;
    ensure(affinity==WDA_EXCLUDEFROMCAPTURE.0,"Capture exclusion readback failed; refusing recursive capture")?;
    let presenter=Presenter::new(hwnd,&factory,&pipe.device,width,height)?;
    let mut capture=Capture{duplication:output.DuplicateOutput(&pipe.device)?,cache:None,rect:desc.DesktopCoordinates,frames:0};
    if let Some(dir)=options.snapshots.as_deref(){std::fs::create_dir(dir)?;}
    let first=Instant::now();
    while !capture.acquire(&pipe)? {ensure(first.elapsed()<Duration::from_secs(3),"No initial desktop frame within 3 seconds")?;std::thread::sleep(Duration::from_millis(10));}
    capture.crop(&pipe,position)?;pipe.prepare();pipe.render(options.dark,0.,true);presenter.present(&pipe)?;
    let _=ShowWindow(hwnd,SW_SHOWNOACTIVATE);
    eprintln!("[glass-live] READY pid={}; hwnd=0x{:X}; capsule={}x{}; dpi={}; no microphone; capture-excluded; right-click closes",
        std::process::id(),hwnd.0 as usize,width,height,dpi);
    eprintln!("[glass-live] left drag=move, left click=theme, middle click=local snapshot if enabled; lifetime={}s",options.seconds);
    let start=Instant::now();let mut dark=options.dark;let mut rendered=1u64;let mut snapshots=0u32;
    let mut msg=MSG::default();let mut done=false;
    let outcome=(||->AppResult<()>{
        while !done&&start.elapsed()<Duration::from_secs(options.seconds){
            let tick=Instant::now();
            for _ in 0..64 {if !PeekMessageW(&mut msg,None,0,0,PM_REMOVE).as_bool(){break;}
                if msg.message==WM_QUIT{done=true;break;}let _=TranslateMessage(&msg);DispatchMessageW(&msg);}
            if done{break;}
            let (move_to,toggle,snapshot,changed)=INPUT.with(|s|{let mut s=s.borrow_mut();
                let v=(s.new_position.take(),s.toggle,s.snapshot,s.display_changed);s.toggle=false;s.snapshot=false;v});
            ensure(!changed,"Display/DPI changed; close and restart the preview on the supported output")?;
            let mut moved=false;
            if let Some(p)=move_to{
                let p=POINT{x:p.x.clamp(work.left+pad,work.right-width as i32-pad),y:p.y.clamp(work.top+pad,work.bottom-height as i32-pad)};
                moved=p.x!=position.x||p.y!=position.y;
                if moved{SetWindowPos(hwnd,None,p.x,p.y,0,0,SWP_NOSIZE|SWP_NOZORDER|SWP_NOACTIVATE)?;position=p;}
            }
            if toggle{dark=!dark;}
            let fresh=capture.acquire(&pipe)?;
            if fresh||moved{capture.crop(&pipe,position)?;pipe.prepare();}
            pipe.render(dark,start.elapsed().as_secs_f32(),true);presenter.present(&pipe)?;rendered+=1;
            if snapshot{if let Some(dir)=options.snapshots.as_deref(){
                ensure(snapshots<100,"Snapshot limit reached (100)")?;snapshots+=1;pipe.snapshot(dir,snapshots)?;
            }else{eprintln!("[glass-live] snapshot ignored: no explicit snapshot directory");}}
            let budget=Duration::from_millis(16);if tick.elapsed()<budget{std::thread::sleep(budget-tick.elapsed());}
        }Ok(())
    })();
    // Remove the window immediately on capture/device loss; never leave stale glass.
    let _=ShowWindow(hwnd,SW_HIDE);
    eprintln!("[glass-live] stopped; captured_frames={}; submitted_frames={}; elapsed_seconds={:.2}; these counters are not display FPS",capture.frames,rendered,start.elapsed().as_secs_f64());
    drop(presenter);drop(capture);drop(pipe);drop(window);
    outcome
}
