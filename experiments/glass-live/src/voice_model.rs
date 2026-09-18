//! Session-scoped presentation, independent of microphone/network/Windows APIs.
//! The voice controller owns business state. This model only owns what is visible.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[repr(u32)]
pub enum Phase {
    #[default]
    Hidden = 0,
    Activating = 1,
    Listening = 2,
    Optimizing = 3,
    Completed = 4,
    Failed = 5,
}
impl Phase {
    pub fn label(self) -> &'static str {
        match self {
            Self::Hidden | Self::Listening => "",
            Self::Activating => "激活中",
            Self::Optimizing => "优化识别中",
            Self::Completed => "已完成",
            Self::Failed => "语音输入失败",
        }
    }
    pub fn can_finish(self) -> bool { matches!(self, Self::Activating | Self::Listening) }
    fn terminal(self) -> bool { matches!(self, Self::Completed | Self::Failed) }
}
#[derive(Clone, Copy, Debug)]
pub struct View {
    pub generation: u64,
    pub enabled: bool,
    pub phase: Phase,
    pub opacity: f32,
    pub dark: bool,
}
impl View {
    pub fn visible(self) -> bool { self.phase != Phase::Hidden && self.opacity > 0.0 }
    pub fn capture_allowed(self) -> bool { self.enabled && self.visible() }
}
#[derive(Debug)]
pub(crate) struct Timeline {
    pub generation: u64,
    pub enabled: bool,
    pub dark: bool,
    phase: Phase,
    dismissed: bool,
    close_at: Option<u64>,
    terminal_hold: bool,
}
const CLOSE_MS: u64 = 180;
impl Timeline {
    pub fn new(enabled: bool, dark: bool) -> Self {
        Self { generation: 0, enabled, dark, phase: Phase::Hidden, dismissed: false,
            close_at: None, terminal_hold: false }
    }
    fn accept(&mut self, generation: u64) -> bool {
        if generation < self.generation { return false; }
        if generation > self.generation {
            self.generation = generation;
            self.phase = Phase::Hidden;
            self.close_at = None;
            self.dismissed = false;
            self.terminal_hold = false;
        }
        true
    }
    pub fn phase(&mut self, generation: u64, phase: Phase, now: u64) {
        if !self.accept(generation) { return; }
        if phase == Phase::Hidden {
            // Finished is followed by business Hidden. Keep only the already
            // requested terminal acknowledgement, never resurrect a dismissed bar.
            if self.terminal_hold { return; }
            self.dismissed = true;
            if self.phase != Phase::Hidden && self.close_at.is_none() { self.close_at = Some(now); }
        } else if !self.dismissed && !self.phase.terminal() {
            self.phase = phase;
            self.close_at = None;
        }
    }
    pub fn failed(&mut self, generation: u64, now: u64) {
        if generation != self.generation || self.dismissed || self.phase == Phase::Hidden { return; }
        self.phase = Phase::Failed;
        self.terminal_hold = true;
        self.close_at = Some(now.saturating_add(900));
    }
    pub fn finished(&mut self, generation: u64, now: u64) {
        if generation != self.generation || self.dismissed || self.phase == Phase::Hidden { return; }
        if self.phase == Phase::Failed { return; }
        self.phase = Phase::Completed;
        self.terminal_hold = true;
        self.close_at = Some(now.saturating_add(550));
    }
    pub fn view(&mut self, now: u64) -> View {
        let opacity = match self.close_at {
            Some(at) if now >= at.saturating_add(CLOSE_MS) => {
                self.phase = Phase::Hidden;
                self.dismissed = true;
                self.close_at = None;
                self.terminal_hold = false;
                0.0
            }
            Some(at) if now >= at => {
                let t = (now-at) as f32 / CLOSE_MS as f32;
                1.0-t*t*(3.0-2.0*t)
            }
            _ => 1.0,
        };
        View { generation: self.generation, enabled: self.enabled, phase: self.phase,
            opacity: if self.phase == Phase::Hidden { 0.0 } else { opacity }, dark: self.dark }
    }
}

/// Same amplitude envelope as the production voice bar: 20 actual-audio-driven
/// strokes; silence is flat. Time only shapes an already non-zero audio envelope.
pub fn waveform_height(index: usize, delta: f32, level: f32) -> f32 {
    const A: [f32;20] = [0.24,0.32,0.44,0.58,0.42,0.64,0.88,1.,0.78,0.56,0.92,0.74,0.58,0.68,0.49,0.42,0.35,0.30,0.25,0.20];
    let level = if level.is_finite() { level.clamp(0.,1.) } else { 0. };
    if level == 0. || index >= A.len() { return 3.; }
    let phase = delta * std::f32::consts::TAU;
    let offset = index as f32 * 0.53;
    let seed = ((index * 73 + 19) % 101) as f32 / 101. * std::f32::consts::TAU;
    let primary = ((phase * (0.82 + index as f32 * 0.013) + offset + seed).sin()+1.)*0.5;
    let secondary = ((phase*2.17-offset*0.71+seed*0.37).sin()+1.)*0.5;
    3. + 13.*A[index]*level.sqrt()*(0.24+0.76*(0.62*primary+0.38*secondary))
}

#[cfg(test)] mod tests {
    use super::*;
    #[test] fn no_capture_before_consent_or_while_hidden() {
        let mut t=Timeline::new(false,false);
        assert!(!t.view(0).capture_allowed());
        t.phase(1,Phase::Listening,0);
        assert!(t.view(0).visible()); assert!(!t.view(0).capture_allowed());
        t.enabled=true; assert!(t.view(0).capture_allowed());
        t.enabled=false; assert!(!t.view(0).capture_allowed());
        t.phase(1,Phase::Hidden,0); assert!(!t.view(180).visible());
        t.enabled=true; assert!(!t.view(1000).capture_allowed());
    }
    #[test] fn terminal_acknowledgement_then_release_and_new_generation_preempts_it() {
        let mut t=Timeline::new(true,false); t.phase(1,Phase::Activating,0);
        t.phase(1,Phase::Listening,20); t.phase(1,Phase::Optimizing,100);
        t.finished(1,150); t.phase(1,Phase::Hidden,151);
        assert_eq!(t.view(160).phase,Phase::Completed);
        assert!(t.view(790).opacity<1.); assert!(!t.view(900).capture_allowed());
        t.phase(2,Phase::Activating,1000); assert_eq!(t.view(1000).phase,Phase::Activating);
        t.finished(1,1001);t.phase(1,Phase::Hidden,1001);
        assert_eq!(t.view(1002).phase,Phase::Activating);
    }
    #[test] fn dismissal_cannot_be_undone_by_late_phase_or_finished() {
        let mut t=Timeline::new(true,false); t.phase(2,Phase::Optimizing,0);
        t.phase(2,Phase::Hidden,10);t.phase(2,Phase::Listening,11);t.finished(2,12);
        assert!(!t.view(200).visible());t.finished(2,1000);assert!(!t.view(1000).visible());
    }
    #[test] fn failed_does_not_turn_into_success_and_restart_cancels_old_deadline() {
        let mut t=Timeline::new(true,false);t.phase(5,Phase::Listening,0);t.failed(5,50);
        t.finished(5,60);t.phase(5,Phase::Hidden,61);assert_eq!(t.view(80).phase,Phase::Failed);
        t.phase(6,Phase::Activating,100);assert_eq!(t.view(2000).phase,Phase::Activating);
    }
    #[test] fn waveform_cannot_invent_audio_or_nonfinite_geometry() {
        for i in 0..20 { for n in 0..100 {
            assert_eq!(waveform_height(i,n as f32,0.),3.);
            assert_eq!(waveform_height(i,n as f32,f32::NAN),3.);
            assert!((3.0..=16.0).contains(&waveform_height(i,n as f32/99.,1.)));
        }}
    }
}
