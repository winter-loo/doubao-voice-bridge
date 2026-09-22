//! Logical controls, visual canvases and capture borders are different spaces.
//! Constants below are product choices, not inferred private Apple parameters.
use std::time::Duration;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct CanvasGeometry {
    pub width: u32,
    pub height: u32,
    pub margin: u32,
}
impl CanvasGeometry {
    pub fn new(width: u32, height: u32) -> Self {
        // 0.225h broad shadow sigma + 0.098h vertical offset needs 0.8855h
        // for 3.5 sigma clearance; round up to 0.90h. This visual margin does not enlarge the logical hit shape.
        Self { width, height, margin: (height as f32 * 0.90).ceil() as u32 }
    }
    pub fn canvas_width(self) -> u32 { self.width + 2 * self.margin }
    pub fn canvas_height(self) -> u32 { self.height + 2 * self.margin }
    pub fn capture_safe_margin(self, capture_pad: u32) -> u32 {
        self.margin.max(capture_pad)
    }
    /// Input stays the original logical capsule. No shadow-area hit forwarding.
    pub fn contains(self, x: f32, y: f32) -> bool {
        let r = self.height as f32 * 0.5;
        let cx = x.clamp(r, self.width as f32 - r);
        (x-cx).powi(2) + (y-r).powi(2) <= r*r
    }
}

/// The renderer's clock is armed only after a valid crop AND preparation exist.
/// The capture initialization timeout continues to use its separate wall clock.
#[derive(Default, Debug)]
pub(crate) struct PresentationClock {
    origin: Option<Duration>,
    last: f32,
}
impl PresentationClock {
    pub fn sample(&mut self, now: Duration, prepared: bool) -> Option<f32> {
        if !prepared { return None; }
        let origin = *self.origin.get_or_insert(now);
        self.last = now.saturating_sub(origin).as_secs_f32().max(self.last);
        Some(self.last)
    }
}

#[cfg(test)] mod tests {
    use super::*;
    #[test] fn shadow_space_never_enlarges_the_control_or_hit_shape() {
        for (w,h) in [(108,26),(135,33),(162,39),(216,52),(324,78)] {
            let g=CanvasGeometry::new(w,h);
            assert!(g.canvas_width()>w && g.canvas_height()>h);
            assert!(!g.contains(-1.,h as f32/2.) && !g.contains(w as f32+1.,h as f32/2.));
            assert!(!g.contains(0.,0.) && g.contains(w as f32/2.,h as f32/2.));
            assert_eq!(g.capture_safe_margin(100),100);
            assert!(g.capture_safe_margin(0)>=g.margin);
        }
    }
    #[test] fn capture_wait_does_not_consume_visual_entrance() {
        for delay in [0,80,350,1500] {
            let mut c=PresentationClock::default();
            assert_eq!(c.sample(Duration::from_millis(delay),false),None);
            assert_eq!(c.sample(Duration::from_millis(delay+25),true),Some(0.));
            assert!((c.sample(Duration::from_millis(delay+125),true).unwrap()-0.1).abs()<1e-6);
            assert_eq!(c.sample(Duration::from_millis(delay+50),true),Some(0.1));
        }
    }
}
