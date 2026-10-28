//! Test helpers: a fake clock, a Notification builder and a harness that
//! drives the engine the way the adapters do and plays the adapters' part
//! for timers.
//!
//! ```ignore
//! let mut h = Harness::new();
//! h.send(Event::NotificationReceived(notification(1).summary("hi").build()));
//! let effects = h.advance(secs(5));
//! assert_eq!(closed(&effects), [(1, CloseReason::Expired)]);
//! ```

use std::cell::Cell;
use std::collections::HashMap;
use std::path::PathBuf;
use std::rc::Rc;
use std::time::{Duration, Instant};

use super::{
    Action, Clock, CloseReason, Effect, Engine, EngineConfig, Event, Expiry, IdAllocator, Image,
    Notification, NotificationId, Timer, Urgency,
};

/// A clock that only moves when told to. Clones share the same time.
#[derive(Debug, Clone)]
pub struct FakeClock(Rc<Cell<Instant>>);

impl FakeClock {
    pub fn new() -> Self {
        Self(Rc::new(Cell::new(Instant::now())))
    }

    pub fn advance(&self, by: Duration) {
        self.0.set(self.0.get() + by);
    }
}

impl Default for FakeClock {
    fn default() -> Self {
        Self::new()
    }
}

impl Clock for FakeClock {
    fn now(&self) -> Instant {
        self.0.get()
    }
}

pub fn secs(s: u64) -> Duration {
    Duration::from_secs(s)
}

pub fn millis(ms: u64) -> Duration {
    Duration::from_millis(ms)
}

/// Start building a Notification with the given id and sensible defaults.
pub fn notification(id: NotificationId) -> NotificationBuilder {
    NotificationBuilder(Notification {
        id,
        app_name: "test-app".into(),
        app_icon: String::new(),
        summary: format!("Notification {id}"),
        body: String::new(),
        actions: Vec::new(),
        hints: Default::default(),
        expiry: Expiry::Default,
    })
}

/// Builder for test Notifications. Finish with [`build`](Self::build), or
/// pass it straight to [`Harness::notify`].
#[derive(Debug, Clone)]
pub struct NotificationBuilder(Notification);

impl NotificationBuilder {
    pub fn app(mut self, app_name: &str) -> Self {
        self.0.app_name = app_name.into();
        self
    }

    pub fn summary(mut self, summary: &str) -> Self {
        self.0.summary = summary.into();
        self
    }

    pub fn body(mut self, body: &str) -> Self {
        self.0.body = body.into();
        self
    }

    pub fn action(mut self, key: &str, label: &str) -> Self {
        self.0.actions.push(Action {
            key: key.into(),
            label: label.into(),
        });
        self
    }

    pub fn urgency(mut self, urgency: Urgency) -> Self {
        self.0.hints.urgency = urgency;
        self
    }

    pub fn expires_after(mut self, after: Duration) -> Self {
        self.0.expiry = Expiry::After(after);
        self
    }

    pub fn never_expires(mut self) -> Self {
        self.0.expiry = Expiry::Never;
        self
    }

    pub fn transient(mut self) -> Self {
        self.0.hints.transient = true;
        self
    }

    pub fn progress(mut self, percent: u8) -> Self {
        self.0.hints.progress = Some(percent);
        self
    }

    pub fn image_path(mut self, path: &str) -> Self {
        self.0.hints.image = Some(Image::Path(PathBuf::from(path)));
        self
    }

    pub fn build(self) -> Notification {
        self.0
    }
}

impl From<NotificationBuilder> for Notification {
    fn from(builder: NotificationBuilder) -> Self {
        builder.build()
    }
}

/// Drives an [`Engine`] with a [`FakeClock`] and keeps track of the timers
/// it schedules, so [`advance`](Self::advance) can fire them on time.
pub struct Harness {
    pub engine: Engine,
    pub clock: FakeClock,
    timers: HashMap<Timer, Instant>,
}

impl Harness {
    pub fn new() -> Self {
        Self::with_config(EngineConfig::default())
    }

    pub fn with_config(config: EngineConfig) -> Self {
        let clock = FakeClock::new();
        Self {
            // Ids the engine allocates itself (restored History entries)
            // start high so they never collide with the ids tests pick.
            engine: Engine::new(config, Box::new(clock.clone()))
                .with_ids(IdAllocator::starting_at(10_000)),
            clock,
            timers: HashMap::new(),
        }
    }

    /// Send one event and return the effects it produced.
    pub fn send(&mut self, event: Event) -> Vec<Effect> {
        let effects = self.engine.handle(event);
        let now = self.clock.now();
        for effect in &effects {
            match effect {
                Effect::ScheduleTimer { timer, after } => {
                    self.timers.insert(*timer, now + *after);
                }
                Effect::CancelTimer(timer) => {
                    self.timers.remove(timer);
                }
                _ => {}
            }
        }
        effects
    }

    /// Shorthand for sending [`Event::NotificationReceived`].
    pub fn notify(&mut self, notification: impl Into<Notification>) -> Vec<Effect> {
        self.send(Event::NotificationReceived(notification.into()))
    }

    /// Move time forward by `by`, firing every pending timer that falls due
    /// on the way, in deadline order. Returns all the effects produced.
    pub fn advance(&mut self, by: Duration) -> Vec<Effect> {
        let target = self.clock.now() + by;
        let mut effects = Vec::new();
        while let Some((timer, due)) = self
            .timers
            .iter()
            .filter(|(_, due)| **due <= target)
            .min_by_key(|(timer, due)| (**due, **timer))
            .map(|(timer, due)| (*timer, *due))
        {
            self.timers.remove(&timer);
            let now = self.clock.now();
            if due > now {
                self.clock.advance(due - now);
            }
            effects.extend(self.send(Event::TimerFired(timer)));
        }
        let now = self.clock.now();
        if target > now {
            self.clock.advance(target - now);
        }
        effects
    }

    /// Timers currently pending, as the adapter would hold them.
    pub fn pending_timers(&self) -> Vec<Timer> {
        let mut timers: Vec<Timer> = self.timers.keys().copied().collect();
        timers.sort();
        timers
    }
}

impl Default for Harness {
    fn default() -> Self {
        Self::new()
    }
}

/// The ids of Popups shown, in order.
pub fn shown(effects: &[Effect]) -> Vec<NotificationId> {
    effects
        .iter()
        .filter_map(|e| match e {
            Effect::ShowPopup(popup) => Some(popup.id()),
            _ => None,
        })
        .collect()
}

/// The ids of Popups updated in place, in order.
pub fn updated(effects: &[Effect]) -> Vec<NotificationId> {
    effects
        .iter()
        .filter_map(|e| match e {
            Effect::UpdatePopup(popup) => Some(popup.id()),
            _ => None,
        })
        .collect()
}

/// The Popups removed from screen, with their reasons, in order.
pub fn removed(effects: &[Effect]) -> Vec<(NotificationId, CloseReason)> {
    effects
        .iter()
        .filter_map(|e| match e {
            Effect::RemovePopup { id, reason } => Some((*id, *reason)),
            _ => None,
        })
        .collect()
}

/// The `NotificationClosed` signals emitted, in order.
pub fn closed(effects: &[Effect]) -> Vec<(NotificationId, CloseReason)> {
    effects
        .iter()
        .filter_map(|e| match e {
            Effect::NotificationClosed { id, reason } => Some((*id, *reason)),
            _ => None,
        })
        .collect()
}

/// The last History count reported in a status change, if any.
pub fn history_count(effects: &[Effect]) -> Option<usize> {
    effects.iter().rev().find_map(|e| match e {
        Effect::StatusChanged(status) => Some(status.history_count),
        _ => None,
    })
}

/// The last Overflow count reported, if any.
pub fn overflow_count(effects: &[Effect]) -> Option<usize> {
    effects.iter().rev().find_map(|e| match e {
        Effect::OverflowChanged { count } => Some(*count),
        _ => None,
    })
}
