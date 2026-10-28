//! Motion: the easing curves and durations from the design, and a small
//! tween driven by frame time. Reduced motion turns every duration into a
//! near-instant jump.

use std::time::Duration;

/// A CSS `cubic-bezier(x1, y1, x2, y2)` timing function.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CubicBezier {
    x1: f64,
    y1: f64,
    x2: f64,
    y2: f64,
}

impl CubicBezier {
    pub const fn new(x1: f64, y1: f64, x2: f64, y2: f64) -> Self {
        Self { x1, y1, x2, y2 }
    }

    /// Eased progress for linear progress `t` in `0..=1`.
    pub fn at(&self, t: f64) -> f64 {
        let t = t.clamp(0.0, 1.0);
        if t == 0.0 || t == 1.0 {
            return t;
        }
        // Solve x(s) = t for s with Newton steps, then bisection as backup.
        let mut s = t;
        for _ in 0..8 {
            let error = bezier(s, self.x1, self.x2) - t;
            let slope = bezier_slope(s, self.x1, self.x2);
            if error.abs() < 1e-7 {
                return bezier(s, self.y1, self.y2);
            }
            if slope.abs() < 1e-6 {
                break;
            }
            s -= error / slope;
        }
        let (mut lo, mut hi) = (0.0, 1.0);
        s = t;
        for _ in 0..40 {
            if bezier(s, self.x1, self.x2) < t {
                lo = s;
            } else {
                hi = s;
            }
            s = (lo + hi) / 2.0;
        }
        bezier(s, self.y1, self.y2)
    }
}

fn bezier(s: f64, p1: f64, p2: f64) -> f64 {
    let u = 1.0 - s;
    3.0 * u * u * s * p1 + 3.0 * u * s * s * p2 + s * s * s
}

fn bezier_slope(s: f64, p1: f64, p2: f64) -> f64 {
    let u = 1.0 - s;
    3.0 * u * u * p1 + 6.0 * u * s * (p2 - p1) + 3.0 * s * s * (1.0 - p2)
}

/// A Popup slides in with a slight overshoot.
pub const ENTER: (Duration, CubicBezier) = (
    Duration::from_millis(380),
    CubicBezier::new(0.2, 0.9, 0.25, 1.12),
);
/// A Popup fades and slides out, accelerating.
pub const LEAVE: (Duration, CubicBezier) = (
    Duration::from_millis(220),
    CubicBezier::new(0.4, 0.0, 1.0, 1.0),
);
/// The rest of the stack glides into place.
pub const REFLOW: (Duration, CubicBezier) = (
    Duration::from_millis(340),
    CubicBezier::new(0.2, 0.8, 0.2, 1.0),
);
/// CSS `ease`, used by the progress ring.
pub const EASE: CubicBezier = CubicBezier::new(0.25, 0.1, 0.25, 1.0);
/// The progress ring glides to each new value.
pub const RING: (Duration, CubicBezier) = (Duration::from_millis(350), EASE);

/// Scale a Popup starts entering from.
pub const ENTER_SCALE: f64 = 0.97;
/// Scale a Popup shrinks to while leaving.
pub const LEAVE_SCALE: f64 = 0.98;

/// What a duration becomes with reduced motion: effectively instant.
const REDUCED: Duration = Duration::from_millis(1);

/// A value moving from `from` to `to` over a duration, timed in frame
/// microseconds.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Tween {
    from: f64,
    to: f64,
    start_us: i64,
    duration_us: i64,
    curve: CubicBezier,
}

impl Tween {
    pub fn new(
        from: f64,
        to: f64,
        start_us: i64,
        (duration, curve): (Duration, CubicBezier),
        reduced: bool,
    ) -> Self {
        let duration = if reduced { REDUCED } else { duration };
        Self {
            from,
            to,
            start_us,
            duration_us: duration.as_micros().max(1) as i64,
            curve,
        }
    }

    /// A tween that has already arrived at `value`, whatever the time.
    pub fn settled(value: f64) -> Self {
        Self::new(value, value, i64::MIN / 2, (REDUCED, EASE), true)
    }

    pub fn value(&self, now_us: i64) -> f64 {
        self.from + (self.to - self.from) * self.curve.at(self.progress(now_us))
    }

    pub fn target(&self) -> f64 {
        self.to
    }

    pub fn done(&self, now_us: i64) -> bool {
        self.progress(now_us) >= 1.0
    }

    /// Head for a new target from wherever the tween is now.
    pub fn retarget(
        &self,
        to: f64,
        now_us: i64,
        timing: (Duration, CubicBezier),
        reduced: bool,
    ) -> Self {
        Self::new(self.value(now_us), to, now_us, timing, reduced)
    }

    fn progress(&self, now_us: i64) -> f64 {
        ((now_us - self.start_us) as f64 / self.duration_us as f64).clamp(0.0, 1.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_linear_curve_is_the_identity() {
        let linear = CubicBezier::new(0.0, 0.0, 1.0, 1.0);
        for t in [0.0, 0.1, 0.5, 0.9, 1.0] {
            assert!((linear.at(t) - t).abs() < 1e-6, "{t}");
        }
    }

    #[test]
    fn css_ease_matches_known_values() {
        // Reference values for cubic-bezier(.25,.1,.25,1) from browsers.
        assert!((EASE.at(0.5) - 0.8024).abs() < 1e-3);
        assert!((EASE.at(0.25) - 0.4094).abs() < 1e-3);
    }

    #[test]
    fn enter_overshoots_before_settling() {
        let (_, curve) = ENTER;
        let peak = (1..100)
            .map(|i| curve.at(f64::from(i) / 100.0))
            .fold(0.0, f64::max);
        assert!(peak > 1.0, "{peak}");
        assert_eq!(curve.at(1.0), 1.0);
    }

    #[test]
    fn a_tween_moves_from_start_to_target_over_its_duration() {
        let tween = Tween::new(0.0, 100.0, 1_000, REFLOW, false);
        assert_eq!(tween.value(1_000), 0.0);
        assert!(!tween.done(1_000 + 339_000));
        assert_eq!(tween.value(1_000 + 340_000), 100.0);
        assert!(tween.done(1_000 + 340_000));
    }

    #[test]
    fn reduced_motion_arrives_almost_at_once() {
        let tween = Tween::new(0.0, 100.0, 0, ENTER, true);
        assert!(tween.done(1_000));
        assert_eq!(tween.value(1_000), 100.0);
    }

    #[test]
    fn a_settled_tween_is_done_at_any_frame_time() {
        let tween = Tween::settled(3.0);
        for now in [0, 1, 1_000_000_000] {
            assert!(tween.done(now), "{now}");
            assert_eq!(tween.value(now), 3.0);
        }
    }

    #[test]
    fn retargeting_starts_from_the_current_value() {
        let tween = Tween::new(0.0, 100.0, 0, LEAVE, false);
        let midway = tween.value(110_000);
        let next = tween.retarget(0.0, 110_000, REFLOW, false);
        assert_eq!(next.value(110_000), midway);
        assert_eq!(next.target(), 0.0);
    }
}
