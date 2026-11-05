//! Shared wheel scrolling for pixel-based virtual views and custom canvases.
//!
//! Accumulates the full wheel delta and eases toward that exact destination
//! over 120 ms. Reversal discards pending travel in the old direction;
//! bounds, keyboard navigation and scrollbar jumps cancel stale travel.
//! Call `position` when drawing and, while `running`, `anim::request_redraw`.
//! This uses the toolkit's shared animation timer, never a timer per view.
//!
//! This is wheel smoothing, not invented touchpad momentum. FLTK 1.5.23
//! exposes integer deltas without Wayland axis source/stop information. A
//! backend supplying precise pixels with native momentum should apply those
//! pixels directly and `jump`, rather than feeding a second inertia model.

use std::time::Instant;

#[derive(Debug, Clone, Default)]
pub struct Scroll {
    value: f64,
    from: f64,
    target: f64,
    reported: i64,
    start: Option<Instant>,
}

impl Scroll {
    pub fn new() -> Self {
        Self::default()
    }

    /// Immediately adopts an external position (keyboard, scrollbar, location).
    pub fn jump(&mut self, position: i64) {
        self.value = position.max(0) as f64;
        self.from = self.value;
        self.target = self.value;
        self.reported = position.max(0);
        self.start = None;
    }

    /// Add a signed pixel distance; preserve magnitude, not only direction.
    /// `position` is the last painted offset; `max` is the current scroll limit.
    pub fn wheel(&mut self, position: i64, delta: f64, max: i64) -> i64 {
        self.wheel_at(position, delta, max, Instant::now(), crate::anim::enabled())
    }

    /// Current painted offset. A changed external position cancels animation.
    pub fn position(&mut self, position: i64, max: i64) -> i64 {
        self.position_at(position, max, Instant::now(), crate::anim::enabled())
    }

    /// Moves the motion by `delta` pixels, keeping a running animation:
    /// content was inserted or removed above the view (scroll anchoring).
    pub fn shift(&mut self, delta: i64) {
        let d = delta as f64;
        self.value = (self.value + d).max(0.0);
        self.from = (self.from + d).max(0.0);
        self.target = (self.target + d).max(0.0);
        self.reported = (self.reported + delta).max(0);
    }

    pub fn running(&self) -> bool {
        self.start.is_some()
    }

    fn position_at(&mut self, position: i64, max: i64, now: Instant, smooth: bool) -> i64 {
        let max = max.max(0);
        if position != self.reported {
            self.jump(position.clamp(0, max));
        }
        let limit = max as f64;
        self.from = self.from.clamp(0.0, limit);
        self.target = self.target.clamp(0.0, limit);
        if let Some(start) = self.start {
            let t = now.saturating_duration_since(start).as_secs_f64() / 0.12;
            if !smooth || t >= 1.0 {
                self.value = self.target;
                self.start = None;
            } else {
                let ease = 1.0 - (1.0 - t).powi(3);
                self.value = self.from + (self.target - self.from) * ease;
            }
        }
        self.value = self.value.clamp(0.0, limit);
        if (self.target - self.value).abs() < 0.01 {
            self.value = self.target;
            self.start = None;
        }
        self.reported = (self.value.round() as i64).clamp(0, max);
        self.reported
    }

    fn wheel_at(&mut self, position: i64, delta: f64, max: i64, now: Instant, smooth: bool) -> i64 {
        self.position_at(position, max, now, smooth);
        if !delta.is_finite() || delta == 0.0 {
            return self.reported;
        }
        let remaining = self.target - self.value;
        let base = if remaining * delta < 0.0 {
            self.value
        } else {
            self.target
        };
        self.from = self.value;
        self.target = (base + delta).clamp(0.0, max.max(0) as f64);
        self.start = Some(now);
        self.position_at(self.reported, max, now, smooth)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;
    fn later(t: Instant, ms: u64) -> Instant {
        t + Duration::from_millis(ms)
    }

    #[test]
    fn keeps_magnitude_and_accumulated_distance_without_extra_momentum() {
        let t = Instant::now();
        let mut s = Scroll::new();
        assert_eq!(s.wheel_at(0, 114.0, 1000, t, true), 0);
        let p = s.position_at(0, 1000, later(t, 30), true);
        assert!(p > 0 && p < 114);
        s.wheel_at(p, 76.0, 1000, later(t, 30), true);
        assert_eq!(s.position_at(p, 1000, later(t, 200), true), 190);
        assert!(!s.running());
    }

    #[test]
    fn frame_rate_does_not_change_motion_or_distance() {
        let t = Instant::now();
        let mut a = Scroll::new();
        let mut b = Scroll::new();
        a.wheel_at(0, 200.0, 1000, t, true);
        b.wheel_at(0, 200.0, 1000, t, true);
        let mut p = 0;
        for ms in [10, 20, 30, 40, 50] {
            p = a.position_at(p, 1000, later(t, ms), true);
        }
        assert_eq!(
            a.position_at(p, 1000, later(t, 60), true),
            b.position_at(0, 1000, later(t, 60), true)
        );
    }

    #[test]
    fn reversal_moves_back_immediately_instead_of_finishing_old_travel() {
        let t = Instant::now();
        let mut s = Scroll::new();
        s.wheel_at(0, 400.0, 1000, t, true);
        let p = s.position_at(0, 1000, later(t, 30), true);
        s.wheel_at(p, -30.0, 1000, later(t, 30), true);
        let q = s.position_at(p, 1000, later(t, 45), true);
        assert!(q < p);
        assert!((s.position_at(q, 1000, later(t, 200), true) - (p - 30)).abs() <= 1);
    }

    #[test]
    fn external_navigation_and_explicit_jump_cancel_travel() {
        let t = Instant::now();
        let mut s = Scroll::new();
        s.wheel_at(0, 300.0, 1000, t, true);
        assert_eq!(s.position_at(700, 1000, later(t, 20), true), 700);
        assert!(!s.running());
        s.wheel_at(700, 200.0, 1000, later(t, 30), true);
        s.jump(0);
        assert_eq!(s.position_at(0, 1000, later(t, 80), true), 0);
        assert!(!s.running());
    }

    #[test]
    fn clamps_huge_deltas_and_changed_bounds_without_overshoot() {
        let t = Instant::now();
        let mut s = Scroll::new();
        s.wheel_at(0, f64::MAX, 100, t, true);
        let mut p = 0;
        for ms in 1..150 {
            p = s.position_at(p, 100, later(t, ms), true);
            assert!((0..=100).contains(&p));
        }
        assert_eq!(p, 100);
        assert_eq!(s.position_at(p, 20, later(t, 160), true), 20);
        s.wheel_at(20, -1e10, 20, later(t, 170), true);
        assert_eq!(s.position_at(20, 20, later(t, 400), true), 0);
        assert!(!s.running());
    }

    #[test]
    fn reduced_motion_is_immediate_and_invalid_deltas_are_ignored() {
        let t = Instant::now();
        let mut s = Scroll::new();
        assert_eq!(s.wheel_at(0, 114.0, 1000, t, false), 114);
        assert!(!s.running());
        assert_eq!(s.wheel_at(114, f64::NAN, 1000, t, false), 114);
        assert_eq!(s.wheel_at(114, f64::INFINITY, 1000, t, false), 114);
        assert_eq!(s.wheel_at(114, 100.0, 0, t, true), 0);
        assert!(!s.running());
    }
}
