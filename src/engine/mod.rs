//! The core engine: every bit of Lucent's behaviour, with no GTK and no
//! D-Bus in sight.
//!
//! The engine is a deterministic state machine. Adapters feed it
//! [`Event`]s and apply the [`Effect`]s it returns, in order. Time comes
//! only from the injected [`Clock`]; waiting is expressed as
//! [`Effect::ScheduleTimer`], which comes back as [`Event::TimerFired`].
//!
//! # Layout
//!
//! Each behaviour lives in its own submodule, as an `impl Engine` block
//! plus that feature's own state and settings types, with its tests next
//! to it. [`Engine::handle`] only dispatches.
//!
//! | module      | owns                                                   |
//! |-------------|--------------------------------------------------------|
//! | `lifecycle` | receiving, replacing and closing Notifications          |
//! | `stack`     | visible Popups and Overflow                             |
//! | `expiry`    | timeouts, expiry timers, hover pause                    |
//! | `actions`   | Actions and click semantics                             |
//! | `history`   | History, restore and clear                              |
//! | `dnd`       | Do Not Disturb                                          |
//! | `rules`     | per-app Rules                                           |
//! | `status`    | the status shown by `lucent status` and Waybar          |
//!
//! Tests drive the engine through [`testing::Harness`] and assert on
//! effects only, never on internal state.

mod actions;
mod clock;
mod config;
mod dnd;
mod effect;
mod event;
mod expiry;
mod history;
mod ids;
mod lifecycle;
mod notification;
mod rules;
mod stack;
mod status;

#[cfg(test)]
pub mod testing;

pub use clock::{Clock, SystemClock};
pub use config::{EngineConfig, Timeouts};
pub use dnd::DndConfig;
pub use effect::{CloseReason, Effect, Popup, Status, Timer};
pub use event::{DndChange, Event, MouseButton};
pub use history::{HistoryConfig, HistoryEntry};
pub use ids::IdAllocator;
pub use notification::{
    Action, Expiry, Hints, Image, ImageData, Notification, NotificationId, UnknownUrgency, Urgency,
};
pub use rules::{Pattern, Rule, RuleTimeout, Rules};

/// Lucent's behaviour. See the [module docs](self).
pub struct Engine {
    config: EngineConfig,
    clock: Box<dyn Clock>,
    stack: stack::Stack,
    expiry: expiry::Deadlines,
    history: history::History,
    dnd: dnd::Dnd,
    /// Fresh ids for restored History entries.
    ids: IdAllocator,
}

impl Engine {
    pub fn new(config: EngineConfig, clock: Box<dyn Clock>) -> Self {
        Self {
            config,
            clock,
            stack: Default::default(),
            expiry: Default::default(),
            history: Default::default(),
            dnd: Default::default(),
            ids: IdAllocator::new(),
        }
    }

    /// Share the D-Bus adapter's id allocator, so restored Popups never
    /// reuse an id an app was given.
    pub fn with_ids(mut self, ids: IdAllocator) -> Self {
        self.ids = ids;
        self
    }

    /// Process one event and return the effects to apply, in order.
    pub fn handle(&mut self, event: Event) -> Vec<Effect> {
        let mut out = Vec::new();
        match event {
            Event::NotificationReceived(notification) => self.receive(notification, &mut out),
            Event::AppClosed(id) => self.close(id, CloseReason::ClosedByApp, &mut out),
            Event::UserDismissed(id) => self.close(id, CloseReason::Dismissed, &mut out),
            Event::UserDismissedAll => self.dismiss_all(&mut out),
            Event::ActionInvoked { id, key } => self.invoke_action(id, key, &mut out),
            Event::PopupClicked { id, button } => self.popup_clicked(id, button, &mut out),
            Event::TimerFired(timer) => self.timer_fired(timer, &mut out),
            Event::HoverStarted(id) => self.hover_started(id, &mut out),
            Event::HoverEnded(id) => self.hover_ended(id, &mut out),
            Event::DndSet(change) => self.set_dnd(change, &mut out),
            Event::HistoryRestored(id) => self.restore_from_history(id, &mut out),
            Event::HistoryCleared => self.clear_history(&mut out),
            Event::OverflowAdvanced => self.advance_overflow(&mut out),
            Event::ConfigReplaced(config) => self.replace_config(config, &mut out),
        }
        out
    }

    fn replace_config(&mut self, config: EngineConfig, out: &mut Vec<Effect>) {
        self.config = config;
        self.fill_from_overflow(out);
        self.apply_history_limit(out);
    }
}
