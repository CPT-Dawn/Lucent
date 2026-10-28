//! History: Lucent's in-memory record of received Notifications.
//!
//! Not implemented yet: ticket #9 (History and the control interface).
//! The hooks below are where it plugs into the engine.

use super::{Effect, Engine, Notification, NotificationId};

/// History settings (`[history]` in the config).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HistoryConfig {
    /// At most this many entries are kept; the oldest drop first.
    pub limit: usize,
}

impl Default for HistoryConfig {
    fn default() -> Self {
        Self { limit: 100 }
    }
}

/// History state.
#[derive(Debug, Default)]
pub(super) struct History {}

impl Engine {
    /// Called for every received Notification that a Rule doesn't keep out
    /// of History, whether or not it gets a Popup.
    pub(super) fn record_in_history(
        &mut self,
        _notification: &Notification,
        _out: &mut Vec<Effect>,
    ) {
        let _ = &self.history;
    }

    pub(super) fn restore_from_history(
        &mut self,
        _id: Option<NotificationId>,
        _out: &mut Vec<Effect>,
    ) {
    }

    pub(super) fn clear_history(&mut self, _out: &mut Vec<Effect>) {}
}
