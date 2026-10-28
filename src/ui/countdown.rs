//! The countdown line's state: how much of a Popup's timeout is left,
//! frozen while the engine says the countdown is paused.

use std::time::{Duration, Instant};

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Countdown {
    total: Duration,
    remaining: Duration,
    /// When `remaining` was measured, if the countdown is running.
    since: Option<Instant>,
}

impl Countdown {
    /// A full countdown, running from `now`.
    pub fn start(total: Duration, now: Instant) -> Self {
        Self {
            total,
            remaining: total,
            since: Some(now),
        }
    }

    pub fn pause(&mut self, remaining: Duration) {
        self.remaining = remaining.min(self.total);
        self.since = None;
    }

    pub fn resume(&mut self, remaining: Duration, now: Instant) {
        self.remaining = remaining.min(self.total);
        self.since = Some(now);
    }

    pub fn is_running(&self) -> bool {
        self.since.is_some()
    }

    /// The share of the timeout left at `now`, from 1 (full) to 0.
    pub fn fraction(&self, now: Instant) -> f64 {
        if self.total.is_zero() {
            return 0.0;
        }
        let elapsed = self
            .since
            .map_or(Duration::ZERO, |since| now.saturating_duration_since(since));
        self.remaining.saturating_sub(elapsed).as_secs_f64() / self.total.as_secs_f64()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ms(n: u64) -> Duration {
        Duration::from_millis(n)
    }

    #[test]
    fn shrinks_linearly_to_zero_over_the_timeout() {
        let t0 = Instant::now();
        let c = Countdown::start(ms(4_000), t0);
        assert_eq!(c.fraction(t0), 1.0);
        assert_eq!(c.fraction(t0 + ms(1_000)), 0.75);
        assert_eq!(c.fraction(t0 + ms(9_000)), 0.0);
    }

    #[test]
    fn holds_still_while_paused_and_carries_on_when_resumed() {
        let t0 = Instant::now();
        let mut c = Countdown::start(ms(4_000), t0);
        c.pause(ms(3_000));
        assert!(!c.is_running());
        assert_eq!(c.fraction(t0 + ms(60_000)), 0.75);

        c.resume(ms(3_000), t0 + ms(60_000));
        assert_eq!(c.fraction(t0 + ms(61_000)), 0.5);
    }
}
