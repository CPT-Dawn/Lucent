//! The Notification core: a pure state machine with no Wayland or D-Bus inside.
//!
//! Adapters translate the outside world into [`Event`]s, feed them to
//! [`Core::handle`], and carry out the returned [`Effect`]s in order. This is
//! Lucent's main test seam.

use std::time::Duration;

use crate::config::Config;

/// A Notification as an application asked for it, already stripped of any
/// transport types.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Notification {
    pub app_name: String,
    pub summary: String,
    pub body: String,
}

/// What the renderer should show for one Notification.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Popup {
    pub id: u32,
    pub app_name: String,
    pub summary: String,
    pub body: String,
}

/// Why a Notification closed, as numbered by the freedesktop spec.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u32)]
pub enum CloseReason {
    Expired = 1,
}

/// Something that happened outside the core.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Event {
    Notify(Notification),
    /// The timer started by [`Effect::StartTimer`] for this id has elapsed.
    TimerFired(u32),
}

/// Something the adapters must do, in the order given.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Effect {
    /// The Notify was accepted under this id; reply to the sender with it.
    Accepted(u32),
    /// The Stack changed; this is the complete set of Popups to show, in order.
    Stack(Vec<Popup>),
    /// Fire [`Event::TimerFired`] for `id` once `after` has elapsed.
    StartTimer { id: u32, after: Duration },
    /// Emit `NotificationClosed` for `id`.
    Closed { id: u32, reason: CloseReason },
}

#[derive(Debug, Default)]
pub struct Core {
    config: Config,
    last_id: u32,
    stack: Vec<Popup>,
}

impl Core {
    /// A core running on the built-in defaults.
    pub fn new() -> Self {
        Self::default()
    }

    /// A core running on the user's resolved config.
    pub fn with_config(config: Config) -> Self {
        Self {
            config,
            ..Self::default()
        }
    }

    pub fn handle(&mut self, event: Event) -> Vec<Effect> {
        match event {
            Event::Notify(n) => self.notify(n),
            Event::TimerFired(id) => self.expire(id),
        }
    }

    fn notify(&mut self, n: Notification) -> Vec<Effect> {
        self.last_id = self.last_id.wrapping_add(1).max(1);
        let id = self.last_id;
        self.stack.push(Popup {
            id,
            app_name: n.app_name,
            summary: n.summary,
            body: n.body,
        });
        vec![
            Effect::Accepted(id),
            Effect::Stack(self.stack.clone()),
            Effect::StartTimer {
                id,
                after: self.config.general.timeout,
            },
        ]
    }

    fn expire(&mut self, id: u32) -> Vec<Effect> {
        let before = self.stack.len();
        self.stack.retain(|p| p.id != id);
        if self.stack.len() == before {
            return vec![];
        }
        vec![
            Effect::Stack(self.stack.clone()),
            Effect::Closed {
                id,
                reason: CloseReason::Expired,
            },
        ]
    }
}
