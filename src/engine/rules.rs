//! Per-app Rules.
//!
//! Not implemented yet: ticket #11 (Per-app Rules). The hook below is
//! where Rules plug into the receive pipeline; they run before History and
//! Do Not Disturb.

use super::{Engine, Notification};

/// The configured Rules, in file order (`[[rule]]` in the config).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Rules {}

/// What the matching Rule decided beyond changing the Notification itself.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct RuleOutcome {
    /// Show a Popup (`hide = false`).
    pub show: bool,
    /// Record in History (`history = true`).
    pub record: bool,
}

impl Default for RuleOutcome {
    fn default() -> Self {
        Self {
            show: true,
            record: true,
        }
    }
}

impl Engine {
    /// Apply the first matching Rule. It may change the Notification
    /// (timeout, Urgency) and decides whether it shows and is recorded.
    pub(super) fn apply_rules(&self, _notification: &mut Notification) -> RuleOutcome {
        let _ = &self.config.rules;
        RuleOutcome::default()
    }
}
