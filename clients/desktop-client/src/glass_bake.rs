//! One worker, one pending request, one completed value. No GPU objects cross
//! this boundary. A newer generation invalidates an in-flight result, even for
//! an A -> B -> A key sequence. The caller never waits for pixel generation.

use std::{
    io,
    panic::{AssertUnwindSafe, catch_unwind},
    sync::{Arc, Condvar, Mutex},
    thread,
};

struct State<K, V> {
    generation: u64,
    target: Option<(u64, K)>,
    pending: Option<(u64, K)>,
    ready: Option<(u64, K, V)>,
    shutdown: bool,
}

pub struct LatestBake<K, V> {
    shared: Arc<(Mutex<State<K, V>>, Condvar)>,
}

impl<K: Copy + Eq + Send + 'static, V: Send + 'static> LatestBake<K, V> {
    pub fn new(mut bake: impl FnMut(K) -> V + Send + 'static) -> io::Result<Self> {
        let shared = Arc::new((Mutex::new(State {
            generation: 0,
            target: None,
            pending: None,
            ready: None,
            shutdown: false,
        }), Condvar::new()));
        let worker = Arc::clone(&shared);
        thread::Builder::new().name("glass-bake".into()).spawn(move || {
            let (mutex, wake) = &*worker;
            loop {
                let request = {
                    let mut state = mutex.lock().unwrap_or_else(|p| p.into_inner());
                    while state.pending.is_none() && !state.shutdown {
                        state = wake.wait(state).unwrap_or_else(|p| p.into_inner());
                    }
                    if state.shutdown { return; }
                    state.pending.take().expect("pending work after wake")
                };
                // No mutex (and no UI thread) is held while calculating pixels.
                let result = catch_unwind(AssertUnwindSafe(|| bake(request.1)));
                let mut state = mutex.lock().unwrap_or_else(|p| p.into_inner());
                if state.shutdown { return; }
                if state.target == Some(request) {
                    match result {
                        Ok(value) => state.ready = Some((request.0, request.1, value)),
                        Err(_) => eprintln!("[glass] bake failed; retaining the previous texture"),
                    }
                }
            }
        })?;
        Ok(Self { shared })
    }

    pub fn request(&self, key: K) {
        let (mutex, wake) = &*self.shared;
        let mut state = mutex.lock().unwrap_or_else(|p| p.into_inner());
        if state.target.is_some_and(|(_, target)| target == key) { return; }
        state.generation = state.generation.wrapping_add(1);
        let request = (state.generation, key);
        state.target = Some(request);
        state.pending = Some(request);
        let obsolete = state.ready.take();
        drop(state);
        wake.notify_one();
        drop(obsolete);
    }

    pub fn take_ready(&self, key: K) -> Option<V> {
        let mut state = self.shared.0.lock().unwrap_or_else(|p| p.into_inner());
        let (generation, target) = state.target?;
        if target != key { return None; }
        if !state.ready.as_ref().is_some_and(|(g, k, _)| *g == generation && *k == key) {
            return None;
        }
        state.ready.take().map(|(_, _, value)| value)
    }
}

impl<K, V> Drop for LatestBake<K, V> {
    fn drop(&mut self) {
        let (mutex, wake) = &*self.shared;
        let mut state = mutex.lock().unwrap_or_else(|p| p.into_inner());
        state.shutdown = true;
        state.pending = None;
        let obsolete = state.ready.take();
        drop(state);
        wake.notify_one();
        drop(obsolete);
        // Deliberately do not join on the paint thread. In-flight CPU work exits
        // on completion; it owns no Window or other UI object.
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{sync::mpsc, time::{Duration, Instant}};

    fn await_value(worker: &LatestBake<u32, u32>, key: u32) -> u32 {
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            if let Some(value) = worker.take_ready(key) { return value; }
            assert!(Instant::now() < deadline, "bake did not complete");
            thread::sleep(Duration::from_millis(1));
        }
    }

    #[test]
    fn coalesces_pending_work_and_discards_stale_results() {
        let (started, starts) = mpsc::channel();
        let (release, permits) = mpsc::channel();
        let worker = LatestBake::new(move |key| {
            started.send(key).unwrap();
            permits.recv().unwrap();
            key
        }).unwrap();
        worker.request(1);
        assert_eq!(starts.recv_timeout(Duration::from_secs(5)).unwrap(), 1);
        worker.request(2);
        worker.request(3);
        release.send(()).unwrap();
        assert_eq!(starts.recv_timeout(Duration::from_secs(5)).unwrap(), 3);
        assert_eq!(worker.take_ready(1), None);
        assert_eq!(worker.take_ready(3), None);
        release.send(()).unwrap();
        assert_eq!(await_value(&worker, 3), 3);
    }

    #[test]
    fn an_aba_key_sequence_still_rejects_the_old_generation() {
        let (started, starts) = mpsc::channel();
        let (release, permits) = mpsc::channel();
        let mut count = 0;
        let worker = LatestBake::new(move |_| {
            count += 1;
            started.send(count).unwrap();
            permits.recv().unwrap();
            count
        }).unwrap();
        worker.request(1);
        assert_eq!(starts.recv_timeout(Duration::from_secs(5)).unwrap(), 1);
        worker.request(2);
        worker.request(1);
        release.send(()).unwrap();
        assert_eq!(starts.recv_timeout(Duration::from_secs(5)).unwrap(), 2);
        assert_eq!(worker.take_ready(1), None);
        release.send(()).unwrap();
        assert_eq!(await_value(&worker, 1), 2);
    }

    #[test]
    fn repeated_requests_do_not_rebake_an_unchanged_key() {
        let calls = Arc::new(std::sync::atomic::AtomicU32::new(0));
        let counter = calls.clone();
        let worker = LatestBake::new(move |key| {
            counter.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            key
        }).unwrap();
        worker.request(4);
        assert_eq!(await_value(&worker, 4), 4);
        for _ in 0..100 { worker.request(4); }
        let state = worker.shared.0.lock().unwrap();
        assert!(state.pending.is_none());
        assert_eq!(calls.load(std::sync::atomic::Ordering::SeqCst), 1);
    }

    #[test]
    fn a_failed_bake_does_not_poison_the_next_request() {
        let (started, starts) = mpsc::channel();
        let worker = LatestBake::new(move |key| {
            started.send(key).unwrap();
            assert_ne!(key, 1, "synthetic bake failure");
            key
        }).unwrap();
        worker.request(1);
        assert_eq!(starts.recv_timeout(Duration::from_secs(5)).unwrap(), 1);
        worker.request(2);
        assert_eq!(await_value(&worker, 2), 2);
    }
}
