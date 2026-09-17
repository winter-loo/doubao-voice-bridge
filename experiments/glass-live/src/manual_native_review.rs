//! Explicitly consented, manual native-window review. Test-only; never linked
//! into the production client. No desktop readback, input injection or audio.
use super::*;
use std::{io::Write, path::PathBuf, sync::{Mutex, OnceLock, mpsc,
    atomic::{AtomicBool, AtomicUsize, Ordering}}};
use windows::Win32::UI::Input::KeyboardAndMouse::GetCapture;

#[derive(Clone, Copy, PartialEq, Eq)]
struct Settings { kind: usize, listening: bool, dark: bool, revision: usize }
struct ReviewState {
    settings: Mutex<Settings>,
    rect: Mutex<Option<(i32, i32, i32, i32, i32)>>,
    stop: AtomicBool,
    failed: AtomicBool,
    painted: AtomicUsize,
    paper_down: AtomicBool,
    body_clicks: AtomicUsize,
    margin_clicks: AtomicUsize,
    moved_margin_clicks: AtomicUsize,
    core_leaks: AtomicUsize,
    paired_moves: AtomicUsize,
}
impl ReviewState {
    fn new() -> Self { Self {
        settings: Mutex::new(Settings { kind: 0, listening: false, dark: false, revision: 1 }),
        rect: Mutex::new(None), stop: AtomicBool::new(false), failed: AtomicBool::new(false),
        painted: AtomicUsize::new(0), paper_down: AtomicBool::new(false),
        body_clicks: AtomicUsize::new(0), margin_clicks: AtomicUsize::new(0),
        moved_margin_clicks: AtomicUsize::new(0), core_leaks: AtomicUsize::new(0),
        paired_moves: AtomicUsize::new(0),
    } }
    fn config(&self) -> Settings { *self.settings.lock().unwrap_or_else(|e| e.into_inner()) }
}
static REVIEW: OnceLock<Arc<ReviewState>> = OnceLock::new();
const BASE: &str = "dbd4d4f0b054c8a462ecf2bb8ee7adc75ddc790c";
// These are BGRA bytes. The paper and GPU texture use the SAME generated color.
fn color(kind: usize) -> [u8; 4] {
    match kind { 1 => [232,232,232,255], 2 => [0,0,0,255],
        3 => [86,183,9,255], _ => [255,255,255,255] }
}
unsafe fn fill(dc: HDC, r: &RECT, c: COLORREF) {
    let brush = CreateSolidBrush(c);
    if !brush.0.is_null() { FillRect(dc, r, brush); let _ = DeleteObject(HGDIOBJ(brush.0)); }
}
unsafe fn label(dc: HDC, mut r: RECT, text: &str) {
    let mut text: Vec<u16> = text.encode_utf16().collect();
    DrawTextW(dc, &mut text, &mut r, DT_CENTER | DT_VCENTER | DT_SINGLELINE);
}
unsafe extern "system" fn paper_proc(h: HWND, m: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    let Some(s) = REVIEW.get() else { return DefWindowProcW(h,m,wp,lp); };
    match m {
        WM_MOUSEACTIVATE => LRESULT(MA_NOACTIVATE as isize),
        WM_ERASEBKGND => LRESULT(1),
        WM_CLOSE | WM_DISPLAYCHANGE | WM_DPICHANGED => {
            if m != WM_CLOSE { s.failed.store(true, Ordering::Release); }
            s.stop.store(true, Ordering::Release); LRESULT(0)
        },
        WM_LBUTTONDOWN => { s.paper_down.store(true, Ordering::Release); LRESULT(0) },
        WM_LBUTTONUP => {
            if !s.paper_down.swap(false, Ordering::AcqRel) { return LRESULT(0); }
            let x = lp.0 as u16 as i16 as i32;
            let y = (lp.0 as u32 >> 16) as u16 as i16 as i32;
            let mut r = RECT::default();
            if GetClientRect(h, &mut r).is_err() { return LRESULT(0); }
            if (0..44).contains(&y) && x >= 0 && x < r.right {
                let button = ((x as i64 * 9) / r.right as i64) as usize;
                let mut config = s.settings.lock().unwrap_or_else(|e| e.into_inner());
                match button {
                    0..=3 => config.kind = button,
                    4 => {}, // replay the selected generated scene
                    5 => config.listening = true,
                    6 => config.listening = false,
                    7 => config.dark = !config.dark,
                    _ => s.stop.store(true, Ordering::Release),
                }
                config.revision += 1;
            } else {
                let mut p = POINT { x, y };
                if ClientToScreen(h, &mut p).as_bool() {
                    if let Some((bx,by,w,hh,margin)) = *s.rect.lock().unwrap_or_else(|e| e.into_inner()) {
                        let in_canvas = p.x >= bx-margin && p.x < bx+w+margin
                            && p.y >= by-margin && p.y < by+hh+margin;
                        let in_body_box = p.x >= bx && p.x < bx+w && p.y >= by && p.y < by+hh;
                        if in_canvas && !in_body_box {
                            s.margin_clicks.fetch_add(1, Ordering::AcqRel);
                            if s.paired_moves.load(Ordering::Acquire) > 0 {
                                s.moved_margin_clicks.fetch_add(1, Ordering::AcqRel);
                            }
                        }
                        if p.x >= bx+hh/2 && p.x < bx+w-hh/2 && p.y >= by+hh/4 && p.y < by+hh*3/4 {
                            s.core_leaks.fetch_add(1, Ordering::AcqRel);
                        }
                    }
                }
            }
            let _ = InvalidateRect(Some(h), None, false); LRESULT(0)
        },
        WM_PAINT => {
            let mut ps = PAINTSTRUCT::default(); let dc = BeginPaint(h, &mut ps);
            let mut r = RECT::default(); let _ = GetClientRect(h, &mut r);
            let config = s.config(); let c = color(config.kind);
            fill(dc, &r, COLORREF(c[2] as u32 | ((c[1] as u32)<<8) | ((c[0] as u32)<<16)));
            let top = RECT { left:0, top:0, right:r.right, bottom:86 };
            let bottom = RECT { left:0, top:r.bottom-42, right:r.right, bottom:r.bottom };
            fill(dc, &top, COLORREF(0x00eeeeee)); fill(dc, &bottom, COLORREF(0x00eeeeee));
            SetBkMode(dc, TRANSPARENT); SetTextColor(dc, COLORREF(0x00222222));
            let old_font = SelectObject(dc, GetStockObject(DEFAULT_GUI_FONT));
            for (i, text) in ["白底", "浅灰", "黑底", "绿底", "重播", "波形", "文字", "浅/深", "退出"].iter().enumerate() {
                label(dc, RECT { left:i as i32*r.right/9, top:0, right:(i as i32+1)*r.right/9, bottom:44 }, text);
            }
            label(dc, RECT { left:0,top:44,right:r.right,bottom:84 },
                "人工审阅：点击胶囊、点击阴影外缘、拖动后再次点击阴影；波形为模拟音量");
            let status = format!("主体点击 {}  阴影穿透 {}  成对移动 {}  移动后穿透 {}  中心漏点 {}",
                s.body_clicks.load(Ordering::Acquire),s.margin_clicks.load(Ordering::Acquire),
                s.paired_moves.load(Ordering::Acquire),s.moved_margin_clicks.load(Ordering::Acquire),
                s.core_leaks.load(Ordering::Acquire));
            label(dc, bottom, &status); SelectObject(dc, old_font);
            let _ = EndPaint(h, &ps);
            s.painted.store(config.revision, Ordering::Release); LRESULT(0)
        },
        _ => DefWindowProcW(h,m,wp,lp),
    }
}
struct PaperClass { instance: HINSTANCE, old: DPI_AWARENESS_CONTEXT }
impl Drop for PaperClass { fn drop(&mut self) { unsafe {
    let _ = UnregisterClassW(w!("DoubaoManualNativeReviewPaper"), Some(self.instance));
    SetThreadDpiAwarenessContext(self.old);
} } }
struct Paper { handle: isize, state: Arc<ReviewState>, thread: Option<std::thread::JoinHandle<()>> }
impl Paper {
    fn start(bounds: RECT, state: Arc<ReviewState>) -> AppResult<Self> {
        let (tx, rx) = mpsc::sync_channel::<Result<isize,String>>(1);
        let worker = state.clone();
        let thread = std::thread::spawn(move || unsafe {
            let result = (|| -> AppResult<()> {
                let old = SetThreadDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);
                ensure(!old.0.is_null(), "Review paper DPI context unavailable")?;
                let instance = match GetModuleHandleW(None) { Ok(m) => HINSTANCE(m.0), Err(e) => {
                    SetThreadDpiAwarenessContext(old); return Err(e.into());
                } };
                let class = PaperClass { instance, old };
                ensure(RegisterClassW(&WNDCLASSW { lpfnWndProc:Some(paper_proc), hInstance:instance,
                    hCursor:LoadCursorW(None,IDC_ARROW)?, lpszClassName:w!("DoubaoManualNativeReviewPaper"),
                    ..Default::default() }) != 0, "Review paper class registration failed")?;
                let h = CreateWindowExW(WS_EX_TOPMOST|WS_EX_TOOLWINDOW|WS_EX_NOACTIVATE,
                    w!("DoubaoManualNativeReviewPaper"),w!("MANUAL REVIEW - generated background only"),WS_POPUP,
                    bounds.left,bounds.top,bounds.right-bounds.left,bounds.bottom-bounds.top,
                    None,None,Some(instance),None)?;
                let window = Window(h);
                let _ = ShowWindow(h,SW_SHOWNOACTIVATE); let _ = UpdateWindow(h);
                tx.send(Ok(h.0 as isize)).map_err(|_| "Review controller disconnected")?;
                let limit = Instant::now();
                while !worker.stop.load(Ordering::Acquire) && limit.elapsed() < Duration::from_secs(210) {
                    let mut msg = MSG::default();
                    for _ in 0..64 { if !PeekMessageW(&mut msg,None,0,0,PM_REMOVE).as_bool() { break; }
                        let _ = TranslateMessage(&msg); DispatchMessageW(&msg);
                    }
                    std::thread::sleep(Duration::from_millis(4));
                }
                worker.stop.store(true,Ordering::Release); drop(window); drop(class); Ok(())
            })();
            if let Err(e) = result {
                worker.failed.store(true,Ordering::Release); worker.stop.store(true,Ordering::Release);
                let _ = tx.try_send(Err(e.to_string())); eprintln!("[manual-native] paper error: {e}");
            }
        });
        let mut paper = Self { handle:0, state, thread:Some(thread) };
        paper.handle = rx.recv_timeout(Duration::from_secs(10))
            .map_err(|_| "Review paper initialization timed out")?
            .map_err(|e| format!("Review paper: {e}"))?;
        Ok(paper)
    }
}
impl Drop for Paper { fn drop(&mut self) {
    self.state.stop.store(true,Ordering::Release);
    if let Some(t) = self.thread.take() { let _ = t.join(); }
} }

unsafe fn native_session(state: Arc<ReviewState>, log: &mut std::fs::File) -> AppResult<(usize,usize,bool)> {
    let context = ThreadContext::new()?;
    // Output/adapter enumeration only. Do NOT construct Session or Capture.
    let (factory,adapter,_output,_desc,monitor) = desktop::choose_output()?;
    let work = monitor.rcWork;
    let pw = (work.right-work.left-40).min(900);
    let ph = (work.bottom-work.top-40).min(430);
    ensure(pw >= 540 && ph >= 260, "Work area too small for bounded manual review")?;
    let left = work.left+(work.right-work.left-pw)/2;
    let top = work.top+(work.bottom-work.top-ph)/2;
    let paper = Paper::start(RECT { left,top,right:left+pw,bottom:top+ph },state.clone())?;
    let paper_hwnd = HWND(paper.handle as *mut _);
    let area = RECT { left:left+8,top:top+96,right:left+pw-8,bottom:top+ph-48 };
    let (body,canvas) = create_pair(&context,POINT { x:left+pw/2,y:top+ph/2 })?;
    let dpi = GetDpiForWindow(body.0); ensure((96..=288).contains(&dpi), "Unsupported review DPI")?;
    let scale = dpi as f32/96.; let w = (108.*scale).round() as u32; let h = (26.*scale).round() as u32;
    let (device,dc) = gpu::create_device(Some(&adapter))?;
    let mut config = state.config();
    let mut phase = Phase::Optimizing;
    let mask = content_mask(w,h,scale,phase)?;
    let mut pipe = Pipeline::new_voice(device.clone(),dc.clone(),w,h,&mask)?;
    let margin = pipe.layout.margin as i32;
    ensure(area.right-area.left > w as i32+2*margin && area.bottom-area.top > h as i32+2*margin,
        "Review area cannot contain the complete visual canvas")?;
    let mut at = clamp_position(POINT { x:left+(pw-w as i32)/2,y:top+(ph-h as i32)/2 },area,w,h,margin);
    configure_pair(body.0,canvas.0,at,pipe.layout,true)?;
    *state.rect.lock().unwrap_or_else(|e|e.into_inner()) = Some((at.x,at.y,w as i32,h as i32,margin));
    crate::voice_render_checks::upload_fixture_checked(&pipe,&color(config.kind).repeat((pipe.raw.width*pipe.raw.height) as usize))?;
    let presenter = Presenter::new(canvas.0,body.0,&factory,&pipe)?; presenter.bind_input(&pipe);
    INPUT.with(|s|*s.borrow_mut()=Input::default());
    let mut start = Instant::now(); let session_start = Instant::now();
    let mut shown = false; let mut submissions = 0; let mut checks = 0;
    writeln!(log,"begin source={BASE} dpi={dpi} body={w}x{h} canvas={}x{} hardware-adapter=true capture=false",pipe.canvas.width,pipe.canvas.height)?; log.flush()?;
    eprintln!("[manual-native] visible review starts; use the paper buttons; auto-close after 180 seconds");
    while !state.stop.load(Ordering::Acquire) && session_start.elapsed() < Duration::from_secs(180) {
        let tick = Instant::now();
        let mut msg = MSG::default();
        for _ in 0..64 { if !PeekMessageW(&mut msg,None,0,0,PM_REMOVE).as_bool() { break; }
            if msg.message == WM_QUIT { state.stop.store(true,Ordering::Release); break; }
            let _ = TranslateMessage(&msg); DispatchMessageW(&msg);
        }
        let (position,click,close,changed) = INPUT.with(|s| {
            let mut s=s.borrow_mut(); let v=(s.position.take(),s.click,s.close,s.display_changed); s.click=false; v
        });
        ensure(!changed && !state.failed.load(Ordering::Acquire), "Display, DPI or paper changed; review stopped")?;
        if close { break; }
        if click { state.body_clicks.fetch_add(1,Ordering::AcqRel); writeln!(log,"body_click")?; }
        if let Some(p) = position {
            let p = clamp_position(p,area,w,h,margin);
            if p.x != at.x || p.y != at.y {
                move_pair(body.0,canvas.0,p,margin)?; at=p;
                *state.rect.lock().unwrap_or_else(|e|e.into_inner())=Some((at.x,at.y,w as i32,h as i32,margin));
                state.paired_moves.fetch_add(1,Ordering::AcqRel);
            }
        }
        let requested=state.config();
        if requested != config {
            if state.painted.load(Ordering::Acquire) < requested.revision {
                std::thread::sleep(Duration::from_millis(4)); continue;
            }
            config=requested; phase=if config.listening { Phase::Listening } else { Phase::Optimizing };
            // Replay prepares fresh generated inputs BEFORE restarting visual time.
            pipe=Pipeline::new_voice(device.clone(),dc.clone(),w,h,&content_mask(w,h,scale,phase)?)?;
            crate::voice_render_checks::upload_fixture_checked(&pipe,&color(config.kind).repeat((pipe.raw.width*pipe.raw.height) as usize))?;
            presenter.bind_input(&pipe); start=Instant::now();
            writeln!(log,"selection kind={} listening={} dark={} revision={}",config.kind,config.listening,config.dark,config.revision)?;
        }
        let age=start.elapsed().as_secs_f32();
        let level=if config.listening { 0.5+0.45*(age*2.5).sin() } else { 0. };
        pipe.render_voice(config.dark,age,phase,level,1.); presenter.present(&pipe)?; submissions+=1;
        if !shown {
            let _=ShowWindow(canvas.0,SW_SHOWNOACTIVATE); let _=ShowWindow(body.0,SW_SHOWNOACTIVATE);
            SetWindowPos(body.0,Some(HWND_TOPMOST),0,0,0,0,SWP_NOMOVE|SWP_NOSIZE|SWP_NOACTIVATE)?; shown=true;
        }
        let mut a=RECT::default(); let mut b=RECT::default();
        GetWindowRect(body.0,&mut a)?; GetWindowRect(canvas.0,&mut b)?;
        ensure((a.left,a.top,a.right-a.left,a.bottom-a.top)==(at.x,at.y,w as i32,h as i32),"Input window geometry mismatch")?;
        ensure((a.left-b.left,a.top-b.top,b.right-b.left,b.bottom-b.top)==(margin,margin,pipe.canvas.width as i32,pipe.canvas.height as i32),"Canvas/body pair diverged")?; checks+=1;
        if submissions%30==0 || click { let _=InvalidateRect(Some(paper_hwnd),None,false); log.flush()?; }
        let _=DwmFlush();
        if let Some(wait)=Duration::from_millis(16).checked_sub(tick.elapsed()) { std::thread::sleep(wait); }
    }
    if GetCapture()==body.0 { let _=ReleaseCapture(); }
    let handles=[body.0,canvas.0,paper_hwnd];
    let _=ShowWindow(body.0,SW_HIDE); let _=ShowWindow(canvas.0,SW_HIDE);
    drop(presenter); drop(pipe); drop(body); drop(canvas); drop(paper);
    let gone=handles.iter().all(|h|!IsWindow(Some(*h)).as_bool());
    ensure(gone,"Owned review windows remain after teardown")?;
    drop(context); Ok((submissions,checks,gone))
}

#[test]
#[ignore = "manual consent required; generated owned windows; no automated mouse input"]
fn local_manual_native_review() {
    let result=(|| -> AppResult<()> {
        ensure(std::env::var("GLASS_MANUAL_NATIVE_REVIEW").as_deref()==Ok("OWNED_WINDOWS_ONLY"),
            "Use the reviewed launcher and confirm in the terminal before showing windows")?;
        ensure(std::env::var_os("GITHUB_ACTIONS").is_none() && std::env::var_os("GLASS_CI_NATIVE_FIXTURE").is_none(),
            "Manual entry refuses hosted-CI flags; it never enables the automatic mouse fixture")?;
        let dir=PathBuf::from(std::env::var_os("GLASS_MANUAL_REVIEW_OUTPUT").ok_or("Missing new report directory")?);
        ensure(dir.is_absolute(),"Report directory must be absolute")?;
        std::fs::create_dir(&dir)?; // never reuse/overwrite an earlier session
        let mut log=std::fs::OpenOptions::new().create_new(true).write(true).open(dir.join("events.log"))?;
        let state=Arc::new(ReviewState::new());
        ensure(REVIEW.set(state.clone()).is_ok(),"Only one manual session per process")?;
        let _serial=WINDOW_TEST_LOCK.lock().unwrap_or_else(|e|e.into_inner());
        let (submissions,checks,gone)=unsafe { native_session(state.clone(),&mut log) }?;
        let body=state.body_clicks.load(Ordering::Acquire); let margin=state.margin_clicks.load(Ordering::Acquire);
        let moved=state.paired_moves.load(Ordering::Acquire); let after=state.moved_margin_clicks.load(Ordering::Acquire);
        let leaks=state.core_leaks.load(Ordering::Acquire);
        let input_pass=body>0 && margin>0 && moved>0 && after>0 && leaks==0;
        let json=format!("{{\n  \"production_source_commit\":\"{BASE}\",\n  \"session_completed\":true,\n  \"presentation_submissions\":{submissions},\n  \"paired_geometry_checks\":{checks},\n  \"body_clicks\":{body},\n  \"margin_clicks\":{margin},\n  \"paired_move_updates\":{moved},\n  \"margin_clicks_after_move\":{after},\n  \"core_click_leaks\":{leaks},\n  \"manual_input_passed\":{input_pass},\n  \"capture_exclusion_checked\":true,\n  \"owned_windows_destroyed\":{gone},\n  \"visual_acceptance\":null,\n  \"desktop_capture\":false,\n  \"audio\":false,\n  \"input_injection\":false,\n  \"scope\":\"Generated owned background, production HWND/presenter path, manual input only. Not an automatic DWM pixel comparison, hardware FPS, voice test or deployment.\"\n}}\n");
        std::fs::write(dir.join("native-session.json"),json)?;
        writeln!(log,"end submissions={submissions} geometry_checks={checks} input_complete={input_pass} windows_destroyed={gone}")?; log.flush()?;
        eprintln!("[manual-native] SESSION COMPLETE; input_complete={input_pass}; visual acceptance requires a separate human report");
        Ok(())
    })();
    if let Err(e)=result { panic!("Manual native review failed: {e}"); }
}
