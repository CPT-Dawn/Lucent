//! Do Not Disturb.
//!
//! Not implemented yet: ticket #10 (Do Not Disturb and live status). The
//! hooks below are where it plugs into the engine.

use super::{DndChange, Effect, Engine, Notification};

/// Do Not Disturb settings (`[dnd]` in the config).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DndConfig {
    /// Critical Notifications still show a Popup while Do Not Disturb is on.
    pub allow_critical: bool,
}

impl Default for DndConfig {
    fn default() -> Self {
        Self {
            allow_critical: true,
        }
    }
}

/// Do Not Disturb state. Always starts off.
#[derive(Debug, Default)]
pub(super) struct Dnd {}

impl Engine {
    /// Whether Do Not Disturb lets this Notification show a Popup.
    pub(super) fn dnd_allows(&self, _notification: &Notification) -> bool {
        let _ = &self.dnd;
        true
    }

    pub(super) fn set_dnd(&mut self, _change: DndChange, _out: &mut Vec<Effect>) {}
}
