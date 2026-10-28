//! Time for the engine. The engine only ever asks its [`Clock`]; it never
//! reads the system time or sleeps, so tests can drive it with a fake.

use std::time::Instant;

/// A source of the current time.
pub trait Clock {
    fn now(&self) -> Instant;
}

/// The real monotonic clock, used by the daemon.
#[derive(Debug, Clone, Copy, Default)]
pub struct SystemClock;

impl Clock for SystemClock {
    fn now(&self) -> Instant {
        Instant::now()
    }
}
