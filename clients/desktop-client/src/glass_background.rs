//! Background decisions, independent of Win32 so the actual sampling policy can
//! be regression-tested without a screen.

pub fn decide_dark_background(
    average_luminance: u32,
    dark_samples: u32,
    valid_samples: u32,
    previous: Option<bool>,
) -> bool {
    if valid_samples == 0 { return previous.unwrap_or(false); }
    let dark = u64::from(dark_samples.min(valid_samples));
    let valid = u64::from(valid_samples);
    match previous {
        None => average_luminance < 150 || dark * 2 >= valid,
        // Enter at 60%, remain dark above 40%. The old unconditional 50%
        // majority return bypassed the luminance hysteresis completely.
        Some(false) => average_luminance < 132 || dark * 5 >= valid * 3,
        Some(true) => average_luminance < 168 || dark * 5 > valid * 2,
    }
}

/// Require two successive samples of a new key before publishing it. Amplitude
/// hysteresis lives in Tint::quantized_near; this adds temporal settling. Include
/// the current key in the candidate so a new session's immediate update resets it.
#[derive(Default)]
pub struct TintSettler {
    pending: Option<(u32, u32)>,
}

impl TintSettler {
    pub fn observe(&mut self, candidate: u32, current: u32) -> u32 {
        if candidate == current {
            self.pending = None;
            return current;
        }
        let pair = (candidate, current);
        if self.pending == Some(pair) {
            self.pending = None;
            candidate
        } else {
            self.pending = Some(pair);
            current
        }
    }
    pub fn reset(&mut self) { self.pending = None; }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn first_look_and_luminance_hysteresis_keep_their_thresholds() {
        assert!(decide_dark_background(140, 2, 14, None));
        assert!(!decide_dark_background(160, 2, 14, None));
        assert!(!decide_dark_background(140, 2, 14, Some(false)));
        assert!(decide_dark_background(160, 2, 14, Some(true)));
        assert!(decide_dark_background(120, 2, 14, Some(false)));
        assert!(!decide_dark_background(175, 2, 14, Some(true)));
    }
    #[test]
    fn one_probe_crossing_148_cannot_bypass_hysteresis() {
        // Six dark samples at 130, one at 147/148, seven at 245. The integer
        // mean stays 188 while the count alternates between seven and six.
        for initial in [false, true] {
            let mut previous = initial;
            for step in 0..100 {
                let probe = if step % 2 == 0 { 147 } else { 148 };
                let mean = (6 * 130 + probe + 7 * 245) / 14;
                let dark = 6 + u32::from(probe < 148);
                previous = decide_dark_background(mean, dark, 14, Some(previous));
                assert_eq!(previous, initial);
            }
        }
    }
    #[test]
    fn a_clear_change_in_dark_fraction_still_switches() {
        assert!(decide_dark_background(180, 9, 14, Some(false)));
        assert!(!decide_dark_background(180, 5, 14, Some(true)));
        assert!(decide_dark_background(188, 7, 14, None));
    }
    #[test]
    fn invalid_probes_preserve_the_previous_state() {
        assert!(decide_dark_background(0, 0, 0, Some(true)));
        assert!(!decide_dark_background(0, 0, 0, Some(false)));
        assert!(!decide_dark_background(0, 0, 0, None));
    }
    #[test]
    fn ratios_do_not_overflow_for_large_counts() {
        assert!(decide_dark_background(200, u32::MAX, u32::MAX, Some(false)));
        assert!(!decide_dark_background(200, 0, u32::MAX, Some(true)));
    }
    #[test]
    fn tint_noise_cannot_publish_alternating_keys() {
        let mut filter = TintSettler::default();
        for i in 0..100 { assert_eq!(filter.observe(0x988 + i % 2, 0x888), 0x888); }
        filter.reset();
        assert_eq!(filter.observe(0x988, 0x888), 0x888);
        assert_eq!(filter.observe(0x988, 0x888), 0x988);
    }
    #[test]
    fn a_new_session_cannot_inherit_the_old_pending_tint() {
        let mut filter = TintSettler::default();
        assert_eq!(filter.observe(0x988, 0x888), 0x888);
        assert_eq!(filter.observe(0x988, 0x777), 0x777);
        filter.reset();
        assert_eq!(filter.observe(0x988, 0x777), 0x777);
        assert_eq!(filter.observe(0x777, 0x777), 0x777);
        assert_eq!(filter.observe(0x988, 0x777), 0x777);
    }
}
