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
