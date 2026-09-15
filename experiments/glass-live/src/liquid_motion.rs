//! Input-driven, bounded springs. No idle wobble or invented audio signal.
#[derive(Clone, Copy, Debug)]
pub struct Motion {
    pub point: [f32; 2],
    pub press: f32,
    pub drift: [f32; 2],
    velocity: [f32; 5],
    pub last_time: f32,
}
impl Default for Motion {
    fn default() -> Self {
        Self { point: [0.30, 0.18], press: 0.0, drift: [0.0; 2], velocity: [0.0; 5], last_time: -1.0 }
    }
}
impl Motion {
    pub fn step(&mut self, time: f32, point: [f32; 2], pressed: bool, drift: [f32; 2], reduced: bool) -> f32 {
        if !time.is_finite() || time < 0.0 || point.iter().chain(drift.iter()).any(|x| !x.is_finite()) { return 0.0; }
        let dt = if self.last_time < 0.0 || time < self.last_time { 1.0 / 60.0 } else { (time-self.last_time).min(0.10) };
        if dt == 0.0 { return 0.0; } // Paired renders must use identical interaction state.
        self.last_time = time;
        let target = [point[0].clamp(0.0,1.0), point[1].clamp(0.0,1.0), if pressed {1.0}else{0.0}, drift[0].clamp(-1.0,1.0), drift[1].clamp(-1.0,1.0)];
        let mut values = [self.point[0],self.point[1],self.press,self.drift[0],self.drift[1]];
        if reduced { values=target; values[3]=0.0; values[4]=0.0; self.velocity=[0.0;5]; }
        else {
            // Substeps keep low-frame-rate recovery stable, not an explicit Euler explosion.
            let steps = (dt / (1.0 / 240.0)).ceil().max(1.0) as u32;
            let h=dt/steps as f32;
            for _ in 0..steps { for i in 0..5 {
                let omega=if i==2 {25.0}else{18.0};
                self.velocity[i] += (omega*omega*(target[i]-values[i])-1.7*omega*self.velocity[i])*h;
                values[i] += self.velocity[i]*h;
            } }
        }
        self.point=[values[0].clamp(0.0,1.0),values[1].clamp(0.0,1.0)];
        self.press=values[2].clamp(0.0,1.04);
        self.drift=[values[3].clamp(-1.0,1.0),values[4].clamp(-1.0,1.0)];
        dt
    }
}
#[cfg(test)] mod tests {
    use super::*;
    #[test] fn press_release_settles_and_same_frame_does_not_advance() {
        let mut s=Motion::default();
        for i in 0..120 { s.step(i as f32/120.0,[0.8,0.7],true,[0.5,-0.4],false); }
        assert!((s.press-1.0).abs()<0.01);
        let old=s; assert_eq!(s.step(s.last_time,[0.0,0.0],false,[0.0;2],false),0.0);
        assert_eq!(old.point,s.point); assert_eq!(old.press,s.press);
        for i in 120..480 { s.step(i as f32/120.0,[0.8,0.7],false,[0.0;2],false); }
        assert!(s.press.abs()<0.0001 && s.drift.iter().all(|v|v.abs()<0.0001));
    }
    #[test] fn reduced_motion_and_long_frames_are_bounded() {
        let mut s=Motion::default();
        for i in 0..100 { s.step(i as f32,[1.0,0.0],i%2==0,[1.0,-1.0],false); assert!(s.press.is_finite() && s.press<=1.04); }
        s.step(101.0,[0.2,0.5],true,[1.0;2],true);
        assert_eq!(s.point,[0.2,0.5]); assert_eq!(s.drift,[0.0;2]);
        let old=s.last_time; s.step(f32::NAN,[0.0;2],false,[0.0;2],false); assert_eq!(old,s.last_time);
    }
}
