//! In-process integration API. The application retains voice, hotkeys and paste.
//! Only an explicit setting enables desktop sampling; Hidden owns no GPU capture.
use crate::voice_model::{Phase, Timeline, View};
use std::sync::{atomic::{AtomicBool, Ordering}, mpsc, Arc, Condvar, Mutex};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

#[derive(Debug)]
pub enum Event {
    Presenting(bool),
    FinishRequested(u64),
    Unavailable(String),
}
#[derive(Clone, Copy)]
pub struct Callbacks {
    pub event: fn(Event),
    /// Reads the application's gated/stale-checked audio level, not microphone I/O.
    pub level: fn() -> f32,
}
#[derive(Debug)]
pub(crate) struct Desired {
    pub timeline: Timeline,
    pub shutdown: bool,
    pub revision: u64,
}
pub(crate) struct Shared {
    pub desired: Mutex<Desired>,
    pub wake: Condvar,
    pub start: Instant,
    pub running: AtomicBool,
    pub presenting: AtomicBool,
}
impl Shared {
    pub fn now(&self) -> u64 { self.start.elapsed().as_millis().min(u64::MAX as u128) as u64 }
    pub fn view(&self) -> (View, bool, u64) {
        let mut d=self.desired.lock().unwrap_or_else(|e|e.into_inner());
        (d.timeline.view(self.now()),d.shutdown,d.revision)
    }
    pub fn allows(&self, generation:u64) -> bool {
        let (view,stop,_)=self.view(); !stop && view.generation==generation && view.capture_allowed()
    }
    pub fn wait(&self, revision:u64) {
        let guard=self.desired.lock().unwrap_or_else(|e|e.into_inner());
        let _guard=self.wake.wait_while(guard,|d| !d.shutdown && d.revision==revision)
            .unwrap_or_else(|e|e.into_inner());
    }
    pub fn active(&self, value:bool, callbacks:Callbacks) {
        if self.presenting.swap(value,Ordering::AcqRel)!=value { (callbacks.event)(Event::Presenting(value)); }
    }
}

pub struct Controller {
    shared: Arc<Shared>,
    join: Mutex<Option<JoinHandle<()>>>,
    done: Mutex<mpsc::Receiver<()>>,
}
impl Controller {
    /// Starts only an idle worker. There is no child EXE, audio device, global
    /// hotkey, network, capture, image saving or settings mutation in this call.
    pub fn new(fallback_hwnd:isize,enabled:bool,dark:bool,callbacks:Callbacks)->Result<Self,String> {
        if fallback_hwnd==0 { return Err("Missing application overlay handle".into()); }
        let shared=Arc::new(Shared{desired:Mutex::new(Desired{timeline:Timeline::new(enabled,dark),shutdown:false,revision:0}),
            wake:Condvar::new(),start:Instant::now(),running:AtomicBool::new(true),presenting:AtomicBool::new(false)});
        let (tx,rx)=mpsc::channel();let state=shared.clone();
        let join=thread::Builder::new().name("voice-liquid-renderer".into()).spawn(move||{
            let result=std::panic::catch_unwind(std::panic::AssertUnwindSafe(||{
                crate::voice_window::run(state.clone(),fallback_hwnd,callbacks)
            }));
            state.running.store(false,Ordering::Release);
            state.active(false,callbacks);
            if result.is_err() {
                crate::voice_window::restore_after_panic(&state,fallback_hwnd);
                (callbacks.event)(Event::Unavailable("Liquid renderer worker panicked; voice controller remains alive".into()));
            }
            let _=tx.send(());
        }).map_err(|e|format!("Could not start liquid renderer: {e}"))?;
        Ok(Self{shared,join:Mutex::new(Some(join)),done:Mutex::new(rx)})
    }
    fn change(&self, f:impl FnOnce(&mut Timeline,u64)) {
        let now=self.shared.now();
        let mut d=self.shared.desired.lock().unwrap_or_else(|e|e.into_inner());
        if d.shutdown {return;}
        f(&mut d.timeline,now); d.revision=d.revision.wrapping_add(1);
        drop(d);self.shared.wake.notify_one();
    }
    pub fn phase(&self,generation:u64,phase:Phase) { self.change(|t,now|t.phase(generation,phase,now)); }
    pub fn finished(&self,generation:u64) { self.change(|t,now|t.finished(generation,now)); }
    pub fn failed(&self,generation:u64) { self.change(|t,now|t.failed(generation,now)); }
    pub fn set_enabled(&self,enabled:bool) { self.change(|t,_|t.enabled=enabled); }
    pub fn set_dark(&self,dark:bool) { self.change(|t,_|t.dark=dark); }
    pub fn enabled(&self)->bool { self.shared.view().0.enabled }
    pub fn owns_visibility(&self)->bool { self.enabled() && self.shared.running.load(Ordering::Acquire) }
    pub fn presenting(&self)->bool { self.shared.presenting.load(Ordering::Acquire) }
    pub fn shutdown(&self) {
        {
            let mut d=self.shared.desired.lock().unwrap_or_else(|e|e.into_inner());
            d.shutdown=true;d.revision=d.revision.wrapping_add(1);
        }
        self.shared.wake.notify_one();
        // Do not block the UI indefinitely in a broken graphics driver. The
        // application owns process shutdown; the normal path joins after cleanup.
        if self.done.lock().unwrap_or_else(|e|e.into_inner()).recv_timeout(Duration::from_secs(2)).is_ok() {
            if let Some(join)=self.join.lock().unwrap_or_else(|e|e.into_inner()).take() {let _=join.join();}
        }
    }
}
impl Drop for Controller { fn drop(&mut self) { self.shutdown(); } }
