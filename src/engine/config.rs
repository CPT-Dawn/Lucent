//! The slice of configuration the engine needs. The config parser builds
//! one of these; the engine never reads files.
//!
//! Each feature keeps its own settings type in its own module so features
//! can grow their settings independently.

use std::time::Duration;

use super::dnd::DndConfig;
use super::history::HistoryConfig;
use super::notification::Urgency;
use super::rules::Rules;

/// Engine settings. `Default` is the spec's out-of-the-box behaviour.
#[derive(Debug, Clone, PartialEq)]
pub struct EngineConfig {
    /// At most this many Popups are visible; the rest wait in Overflow.
    pub max_visible: usize,
    pub timeouts: Timeouts,
    pub history: HistoryConfig,
    pub dnd: DndConfig,
    pub rules: Rules,
}

impl Default for EngineConfig {
    fn default() -> Self {
        Self {
            max_visible: 5,
            timeouts: Timeouts::default(),
            history: HistoryConfig::default(),
            dnd: DndConfig::default(),
            rules: Rules::default(),
        }
    }
}

/// How long a Popup stays when the app leaves it to Lucent, by Urgency.
/// `None` means never expire.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Timeouts {
    pub low: Option<Duration>,
    pub normal: Option<Duration>,
    pub critical: Option<Duration>,
}

impl Timeouts {
    /// The same timeout for every Urgency (v1's `timeout_seconds`).
    pub fn uniform(timeout: Option<Duration>) -> Self {
        Self {
            low: timeout,
            normal: timeout,
            critical: timeout,
        }
    }

    pub fn for_urgency(&self, urgency: Urgency) -> Option<Duration> {
        match urgency {
            Urgency::Low => self.low,
            Urgency::Normal => self.normal,
            Urgency::Critical => self.critical,
        }
    }
}

impl Default for Timeouts {
    fn default() -> Self {
        Self {
            low: Some(Duration::from_secs(3)),
            normal: Some(Duration::from_secs(5)),
            critical: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::{EngineConfig, Timeouts};
    use crate::engine::testing::*;
    use crate::engine::{CloseReason, Event, Urgency};

    fn timeouts(low: u64, normal: u64, critical: Option<u64>) -> Timeouts {
        Timeouts {
            low: Some(secs(low)),
            normal: Some(secs(normal)),
            critical: critical.map(Duration::from_secs),
        }
    }

    fn expired(id: u32) -> Vec<(u32, CloseReason)> {
        vec![(id, CloseReason::Expired)]
    }

    #[test]
    fn configured_urgency_timeouts_replace_the_defaults() {
        let mut h = Harness::with_config(EngineConfig {
            timeouts: timeouts(1, 2, Some(4)),
            ..EngineConfig::default()
        });
        h.notify(notification(1).urgency(Urgency::Low));
        h.notify(notification(2));
        h.notify(notification(3).urgency(Urgency::Critical));

        assert_eq!(closed(&h.advance(secs(1))), expired(1));
        assert_eq!(closed(&h.advance(secs(1))), expired(2));
        assert_eq!(closed(&h.advance(secs(2))), expired(3));
    }

    #[test]
    fn the_apps_own_expiry_still_wins_over_configured_timeouts() {
        let mut h = Harness::with_config(EngineConfig {
            timeouts: timeouts(1, 1, None),
            ..EngineConfig::default()
        });
        h.notify(notification(1).expires_after(secs(10)));
        h.notify(
            notification(2)
                .urgency(Urgency::Critical)
                .expires_after(secs(3)),
        );

        assert!(h.advance(secs(1)).is_empty());
        assert_eq!(closed(&h.advance(secs(2))), expired(2));
        assert_eq!(closed(&h.advance(secs(7))), expired(1));
    }

    #[test]
    fn a_replaced_config_applies_to_notifications_received_afterwards() {
        let mut h = Harness::new();
        h.notify(notification(1));

        h.send(Event::ConfigReplaced(EngineConfig {
            timeouts: timeouts(1, 1, Some(1)),
            ..EngineConfig::default()
        }));
        h.notify(notification(2));

        assert_eq!(closed(&h.advance(secs(1))), expired(2));
        assert_eq!(closed(&h.advance(secs(4))), expired(1));
    }

    #[test]
    fn a_replaced_config_can_shrink_the_visible_stack_for_new_popups() {
        let mut h = Harness::new();
        h.send(Event::ConfigReplaced(EngineConfig {
            max_visible: 1,
            ..EngineConfig::default()
        }));

        h.notify(notification(1));
        let effects = h.notify(notification(2));

        assert!(shown(&effects).is_empty(), "{effects:?}");
        assert_eq!(overflow_count(&effects), Some(1));
    }
}
