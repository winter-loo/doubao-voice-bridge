//! Test-only replay state reset. Reuse compiled shaders/textures in the manual
//! host; never alter the approved production renderer or its live lifecycle.
use super::*;

impl AdaptivePipeline {
    /// Reset the state consumed by AdaptivePipeline::render_voice, not resources.
    /// Source textures, text support, HWND and reduced-motion choice are retained.
    pub(crate) unsafe fn reset_manual_replay(&self) {
        self.context.PSSetShaderResources(0, Some(&[None, None, None, None, None, None]));
        for history in &self.history {
            self.context.ClearRenderTargetView(&history.target, &[0.0; 4]);
        }
        self.index.set(0);
        self.initialized.set(false);
        self.last_time.set(-1.0);
        self.foreground_start.set(0.0);
        self.phase.set(None);
        *self.inner.motion.borrow_mut() = Default::default();
        self.inner.last_position.set(None);
    }
}

#[test]
fn reused_manual_replay_matches_fresh_pipeline() { unsafe {
    use crate::voice_render_checks::upload_fixture_checked;
    use windows::core::Interface;
    let (device, context) = create_device(None).unwrap();
    let (w, h) = (162u32, 39u32);
    let text = crate::voice_window::content_mask(w, h, 1.5, Phase::Optimizing).unwrap();
    let empty = vec![0u8; (w * h) as usize];
    let reused = AdaptivePipeline::new_voice(device.clone(), context.clone(), w, h, &text).unwrap();
    let identities = (
        reused.shader.as_raw(), reused.reduce.as_raw(), reused.surface.as_raw(),
        reused.canvas.texture.as_raw(), reused.history[0].texture.as_raw(),
        reused.history[1].texture.as_raw(),
    );
    // Independent fresh constructors remain a TEST control, never a button path.
    let cases = [
        ([255u8,255,255,255], Phase::Optimizing, false, false),
        ([86u8,183,9,255], Phase::Optimizing, false, false),
        ([0u8,0,0,255], Phase::Listening, true, false),
        ([255u8,255,255,255], Phase::Optimizing, false, true),
    ];
    let mut comparisons = 0;
    for (color, phase, dark, reduced) in cases {
        // Seed stale material/foreground/motion state before every replay.
        reused.inner.reduced_motion.set(false);
        reused.set_voice_mask(&text).unwrap();
        let dirty = [32u8,80,210,255].repeat((reused.raw.width * reused.raw.height) as usize);
        upload_fixture_checked(&reused, &dirty).unwrap();
        reused.render_voice(!dark, 2.5, Phase::Optimizing, 0.0, 1.0);
        reused.render_voice(!dark, 3.0, Phase::Optimizing, 0.0, 1.0);
        reused.inner.last_position.set(Some((123, 456)));
        let mask = if phase == Phase::Listening { &empty } else { &text };
        let fresh = AdaptivePipeline::new_voice(device.clone(), context.clone(), w, h, mask).unwrap();
        let input = color.repeat((reused.raw.width * reused.raw.height) as usize);
        upload_fixture_checked(&fresh, &input).unwrap();
        upload_fixture_checked(&reused, &input).unwrap();
        reused.set_voice_mask(mask).unwrap();
        fresh.inner.reduced_motion.set(reduced);
        reused.inner.reduced_motion.set(reduced);
        reused.reset_manual_replay();
        assert_eq!(reused.inner.reduced_motion.get(), reduced);
        assert_eq!(reused.inner.last_position.get(), None);
        assert_eq!(identities, (
            reused.shader.as_raw(), reused.reduce.as_raw(), reused.surface.as_raw(),
            reused.canvas.texture.as_raw(), reused.history[0].texture.as_raw(),
            reused.history[1].texture.as_raw(),
        ), "Replay replaced an existing shader, buffer or texture");
        for frame in 0..=48 {
            let time = frame as f32 / 60.0;
            let level = if phase == Phase::Listening { 0.65 } else { 0.0 };
            fresh.render_voice(dark, time, phase, level, 1.0);
            reused.render_voice(dark, time, phase, level, 1.0);
            if matches!(frame, 0 | 3 | 6 | 8 | 12 | 18 | 24 | 36 | 48) {
                assert_eq!(fresh.read_rgba(&fresh.canvas).unwrap(), reused.read_rgba(&reused.canvas).unwrap(),
                    "Reused replay differs from fresh production renderer: frame={frame}, dark={dark}, reduced={reduced}");
                comparisons += 1;
            }
        }
        let held = reused.read_rgba(&reused.canvas).unwrap();
        reused.render_voice(dark, 0.8, phase, if phase == Phase::Listening { 0.65 } else { 0.0 }, 1.0);
        assert_eq!(held, reused.read_rgba(&reused.canvas).unwrap(), "Same-time replay render changed history");
    }
    eprintln!("[manual-replay-reset] PASS; {comparisons} fresh-vs-reused canvas comparisons; retained resource identities; reduced motion and same-time checks; generated inputs, no windows");
} }
