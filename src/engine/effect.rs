//! Everything the engine can ask the outside world to do. The GTK adapter
//! handles the Popup effects and timers; the D-Bus adapter handles the
//! signals and status.

use std::time::{Duration, Instant};

use super::notification::{Notification, NotificationId};

/// An output of the engine, in the order the adapters should apply it.
#[derive(Debug, Clone, PartialEq)]
pub enum Effect {
    /// Show a new Popup, newest in the stack.
    ShowPopup(Popup),
    /// Replace the contents of a visible Popup in place.
    UpdatePopup(Popup),
    /// Take a Popup off screen.
    RemovePopup {
        id: NotificationId,
        reason: CloseReason,
    },
    /// The number of Notifications waiting in Overflow changed.
    OverflowChanged { count: usize },
    /// Emit the `NotificationClosed` D-Bus signal.
    NotificationClosed {
        id: NotificationId,
        reason: CloseReason,
    },
    /// Emit the `ActionInvoked` D-Bus signal.
    ActionInvoked { id: NotificationId, key: String },
    /// Fire [`Event::TimerFired`](super::Event::TimerFired) after `after`.
    /// Scheduling a timer that is already pending replaces it.
    ScheduleTimer { timer: Timer, after: Duration },
    /// Forget a pending timer, if any.
    CancelTimer(Timer),
    /// A Popup's countdown froze (it is hovered) with this much time left.
    CountdownPaused {
        id: NotificationId,
        remaining: Duration,
    },
    /// A Popup's countdown carries on with this much time left.
    CountdownResumed {
        id: NotificationId,
        remaining: Duration,
    },
    /// The History count or Do Not Disturb state changed.
    StatusChanged(Status),
}

/// What a Popup shows: the Notification plus what the engine decided
/// about it.
#[derive(Debug, Clone, PartialEq)]
pub struct Popup {
    pub notification: Notification,
    /// When the engine received (or last replaced) the Notification.
    pub received_at: Instant,
    /// How long the Popup stays before expiring; `None` means never.
    pub timeout: Option<Duration>,
}

impl Popup {
    pub fn id(&self) -> NotificationId {
        self.notification.id
    }
}

/// A timer owned by the engine. Each kind is keyed so that at most one of
/// each is pending at a time.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Timer {
    /// The visible Popup for this Notification should expire.
    Expire(NotificationId),
}

/// Why a Notification closed, as defined by the freedesktop spec.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum CloseReason {
    Expired,
    Dismissed,
    ClosedByApp,
    Undefined,
}

impl CloseReason {
    /// The reason code sent in `NotificationClosed`.
    pub fn code(self) -> u32 {
        match self {
            CloseReason::Expired => 1,
            CloseReason::Dismissed => 2,
            CloseReason::ClosedByApp => 3,
            CloseReason::Undefined => 4,
        }
    }
}

/// Live status for the CLI and Waybar.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Status {
    pub history_count: usize,
    pub dnd: bool,
}
