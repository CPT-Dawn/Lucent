//! Everything that can happen to the engine. Adapters translate D-Bus
//! calls, clicks, hover and timers into these; the engine answers with
//! [`Effect`](super::Effect)s.

use super::config::EngineConfig;
use super::effect::Timer;
use super::notification::{Notification, NotificationId};

/// An input to the engine.
#[derive(Debug, Clone, PartialEq)]
pub enum Event {
    /// An app sent a Notification (`Notify`). If its id is already known,
    /// it replaces that Notification.
    NotificationReceived(Notification),
    /// The app closed a Notification (`CloseNotification`).
    AppClosed(NotificationId),
    /// The user dismissed one Popup (close button, CLI).
    UserDismissed(NotificationId),
    /// The user dismissed every Popup (`lucent dismiss --all`).
    UserDismissedAll,
    /// The user invoked an Action on a Popup (an Action button).
    ActionInvoked { id: NotificationId, key: String },
    /// The user clicked a Popup's body. The engine decides what that
    /// means: see [`MouseButton`].
    PopupClicked {
        id: NotificationId,
        button: MouseButton,
    },
    /// A timer the engine scheduled has fired.
    TimerFired(Timer),
    /// The pointer entered a Popup.
    HoverStarted(NotificationId),
    /// The pointer left a Popup.
    HoverEnded(NotificationId),
    /// Do Not Disturb was switched on, off or toggled.
    DndSet(DndChange),
    /// Restore a History entry as a new Popup; `None` means the most recent.
    HistoryRestored(Option<NotificationId>),
    /// History was emptied.
    HistoryCleared,
    /// The "+N more" pill was clicked.
    OverflowAdvanced,
    /// A new config was loaded.
    ConfigReplaced(EngineConfig),
}

/// Which mouse button clicked a Popup.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MouseButton {
    /// Invokes the default Action, or dismisses when there is none.
    Primary,
    /// Dismisses every Popup.
    Middle,
    /// Dismisses this Popup.
    Secondary,
}

/// How a Do Not Disturb request changes the current state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DndChange {
    On,
    Off,
    Toggle,
}
