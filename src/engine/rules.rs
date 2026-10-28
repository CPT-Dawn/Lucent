//! Per-app Rules: user-defined matches on a Notification's app name and/or
//! summary that change how it is handled. They run first in the receive
//! pipeline, before Do Not Disturb and History, so a Rule that raises
//! Urgency to Critical lets a Notification through Do Not Disturb.

use std::time::Duration;

use super::{Engine, Notification, Urgency};

/// The configured Rules, in file order (`[[rule]]` in the config).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Rules(pub Vec<Rule>);

/// One `[[rule]]`: what it matches and what it changes. A Rule matches
/// when every pattern it has matches; the config parser rejects a Rule
/// with no patterns.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Rule {
    /// Matches the Notification's app name.
    pub app: Option<Pattern>,
    /// Matches the Notification's summary.
    pub summary: Option<Pattern>,
    /// Replaces the timeout, even one the app set itself.
    pub timeout: Option<RuleTimeout>,
    /// Replaces the Urgency the app set.
    pub urgency: Option<Urgency>,
    /// Show no Popup (the Notification still enters History).
    pub hide: bool,
    /// Record the Notification in History.
    pub history: bool,
}

impl Default for Rule {
    /// Matches everything and changes nothing.
    fn default() -> Self {
        Self {
            app: None,
            summary: None,
            timeout: None,
            urgency: None,
            hide: false,
            history: true,
        }
    }
}

impl Rule {
    fn matches(&self, notification: &Notification) -> bool {
        self.app
            .as_ref()
            .is_none_or(|p| p.matches(&notification.app_name))
            && self
                .summary
                .as_ref()
                .is_none_or(|p| p.matches(&notification.summary))
    }
}

/// A Rule's `timeout`; `0` in the config means never.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RuleTimeout {
    After(Duration),
    Never,
}

/// A case-insensitive match where `*` stands for any run of characters,
/// including none. Everything else is literal: no regex.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Pattern(String);

impl Pattern {
    pub fn new(pattern: &str) -> Self {
        Self(pattern.to_lowercase())
    }

    pub fn matches(&self, text: &str) -> bool {
        let text = text.to_lowercase();
        let mut parts = self.0.split('*');
        // `split` always yields at least one part: the literal prefix.
        let prefix = parts.next().unwrap_or_default();
        let Some(mut rest) = text.strip_prefix(prefix) else {
            return false;
        };
        let mut parts = parts.peekable();
        while let Some(part) = parts.next() {
            if parts.peek().is_none() {
                // The part after the last `*` is anchored at the end.
                return rest.ends_with(part);
            }
            match rest.find(part) {
                Some(at) => rest = &rest[at + part.len()..],
                None => return false,
            }
        }
        // No `*` at all: the prefix had to be the whole text.
        rest.is_empty()
    }
}

/// What the matching Rule decided beyond changing the Notification itself.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct RuleOutcome {
    /// Show a Popup (`hide = false`).
    pub show: bool,
    /// Record in History (`history = true`).
    pub record: bool,
    /// The Rule's own timeout, which wins over every other expiry.
    pub timeout: Option<RuleTimeout>,
}

impl Default for RuleOutcome {
    fn default() -> Self {
        Self {
            show: true,
            record: true,
            timeout: None,
        }
    }
}

impl Engine {
    /// Apply the first matching Rule. It may change the Notification's
    /// Urgency, and decides its timeout and whether it shows and is
    /// recorded.
    pub(super) fn apply_rules(&self, notification: &mut Notification) -> RuleOutcome {
        let Some(rule) = self.config.rules.0.iter().find(|r| r.matches(notification)) else {
            return RuleOutcome::default();
        };
        if let Some(urgency) = rule.urgency {
            notification.hints.urgency = urgency;
        }
        RuleOutcome {
            show: !rule.hide,
            record: rule.history,
            timeout: rule.timeout,
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::engine::testing::*;
    use crate::engine::{
        CloseReason, DndChange, Effect, EngineConfig, Event, Pattern, Rule, RuleTimeout, Rules,
        Urgency,
    };

    fn with_rules(rules: impl IntoIterator<Item = Rule>) -> Harness {
        Harness::with_config(EngineConfig {
            rules: Rules(rules.into_iter().collect()),
            ..Default::default()
        })
    }

    fn rule(app: &str, summary: &str) -> Rule {
        Rule {
            app: (!app.is_empty()).then(|| Pattern::new(app)),
            summary: (!summary.is_empty()).then(|| Pattern::new(summary)),
            ..Default::default()
        }
    }

    fn hiding(app: &str, summary: &str) -> Rule {
        Rule {
            hide: true,
            ..rule(app, summary)
        }
    }

    #[test]
    fn a_hiding_rule_shows_no_popup_but_still_records_in_history() {
        let mut h = with_rules([hiding("discord", "*is typing*")]);

        let effects = h.notify(notification(1).app("discord").summary("Ana is typing…"));

        assert!(shown(&effects).is_empty(), "{effects:?}");
        assert_eq!(history_count(&effects), Some(1));
        assert!(h.pending_timers().is_empty());
    }

    /// Whether a Rule matching `app` / `summary` hides this Notification.
    fn hides(app_pattern: &str, summary_pattern: &str, app: &str, summary: &str) -> bool {
        let mut h = with_rules([hiding(app_pattern, summary_pattern)]);
        shown(&h.notify(notification(1).app(app).summary(summary))).is_empty()
    }

    #[test]
    fn notifications_no_rule_matches_are_left_alone() {
        let mut h = with_rules([hiding("discord", "*is typing*")]);

        let effects = h.notify(notification(1).app("discord").summary("New message"));

        assert_eq!(shown(&effects), [1]);
        assert_eq!(closed(&h.advance(secs(5))), [(1, CloseReason::Expired)]);
    }

    #[test]
    fn matching_ignores_case() {
        assert!(hides("Discord", "", "discord", "hi"));
        assert!(hides("discord", "", "DISCORD", "hi"));
        assert!(hides("", "*IS TYPING*", "discord", "Ana is typing"));
    }

    #[test]
    fn without_a_wildcard_the_whole_text_must_match() {
        assert!(hides("discord", "", "discord", "hi"));
        assert!(!hides("discord", "", "discord-canary", "hi"));
        assert!(!hides("discord", "", "vesktop discord", "hi"));
    }

    #[test]
    fn a_wildcard_matches_any_run_of_characters() {
        assert!(hides("discord*", "", "discord-canary", "hi"));
        assert!(hides("discord*", "", "discord", "hi"));
        assert!(!hides("discord*", "", "vesktop", "hi"));
        assert!(hides("*cord", "", "discord", "hi"));
        assert!(!hides("*cord", "", "discord-canary", "hi"));
        assert!(hides("", "*is typing*", "x", "Ana is typing…"));
        assert!(hides("", "a*b*c", "x", "a-b-b-c"));
        assert!(!hides("", "a*b*c", "x", "a-c-b"));
        assert!(hides("*", "", "anything", "hi"));
    }

    #[test]
    fn wildcards_are_the_only_special_characters() {
        assert!(!hides("disc.rd", "", "discord", "hi"));
        assert!(hides("", "[1/2] done?", "x", "[1/2] done?"));
    }

    #[test]
    fn a_rule_with_app_and_summary_needs_both_to_match() {
        assert!(hides("discord", "*typing*", "discord", "Ana is typing"));
        assert!(!hides("discord", "*typing*", "slack", "Ana is typing"));
        assert!(!hides("discord", "*typing*", "discord", "New message"));
    }

    #[test]
    fn the_first_matching_rule_wins() {
        let mut h = with_rules([rule("discord", "*typing*"), hiding("discord", "")]);

        let typing = h.notify(notification(1).app("discord").summary("Ana is typing"));
        let message = h.notify(notification(2).app("discord").summary("New message"));

        assert_eq!(shown(&typing), [1], "the first Rule changes nothing");
        assert!(shown(&message).is_empty(), "the second Rule hides it");
    }

    #[test]
    fn a_rule_timeout_replaces_the_urgency_default() {
        let mut h = with_rules([Rule {
            timeout: Some(RuleTimeout::After(secs(2))),
            ..rule("spotify", "")
        }]);
        h.notify(notification(1).app("Spotify").summary("Now playing"));

        assert!(h.advance(millis(1999)).is_empty());
        assert_eq!(closed(&h.advance(millis(1))), [(1, CloseReason::Expired)]);
    }

    #[test]
    fn a_rule_timeout_wins_over_the_apps_own_expiry() {
        let mut h = with_rules([Rule {
            timeout: Some(RuleTimeout::After(secs(2))),
            ..rule("spotify", "")
        }]);
        h.notify(notification(1).app("spotify").expires_after(secs(30)));

        assert_eq!(closed(&h.advance(secs(2))), [(1, CloseReason::Expired)]);
    }

    #[test]
    fn a_rule_timeout_still_applies_to_a_critical_notification() {
        let mut h = with_rules([Rule {
            timeout: Some(RuleTimeout::After(secs(2))),
            ..rule("backup", "")
        }]);
        h.notify(
            notification(1)
                .app("backup")
                .urgency(Urgency::Critical)
                .expires_after(secs(30)),
        );

        assert_eq!(closed(&h.advance(secs(2))), [(1, CloseReason::Expired)]);
    }

    #[test]
    fn a_rule_raising_urgency_to_critical_also_drops_the_apps_expiry() {
        let mut h = with_rules([Rule {
            urgency: Some(Urgency::Critical),
            ..rule("pager", "")
        }]);
        h.notify(notification(1).app("pager").expires_after(secs(2)));

        assert!(h.advance(secs(3_600)).is_empty());
    }

    #[test]
    fn a_rule_timeout_of_never_keeps_the_popup_until_dismissed() {
        let mut h = with_rules([Rule {
            timeout: Some(RuleTimeout::Never),
            ..rule("backup", "")
        }]);
        h.notify(notification(1).app("backup"));

        assert!(h.pending_timers().is_empty());
        assert!(h.advance(secs(3600)).is_empty());
    }

    #[test]
    fn a_rule_can_change_the_urgency() {
        let mut h = with_rules([Rule {
            urgency: Some(Urgency::Critical),
            ..rule("slack", "")
        }]);

        let effects = h.notify(notification(1).app("Slack").summary("Prod is down"));

        let Some(Effect::ShowPopup(popup)) = effects.first() else {
            panic!("expected a Popup, got {effects:?}");
        };
        assert_eq!(popup.notification.hints.urgency, Urgency::Critical);
        assert!(h.advance(secs(3600)).is_empty(), "Critical never expires");
        let newest = h.engine.history().next().expect("recorded");
        assert_eq!(newest.notification.hints.urgency, Urgency::Critical);
    }

    #[test]
    fn a_rule_raising_urgency_to_critical_passes_do_not_disturb() {
        let mut h = with_rules([Rule {
            urgency: Some(Urgency::Critical),
            ..rule("slack", "")
        }]);
        h.send(Event::DndSet(DndChange::On));

        let slack = h.notify(notification(1).app("slack"));
        let other = h.notify(notification(2).app("discord"));

        assert_eq!(shown(&slack), [1]);
        assert!(shown(&other).is_empty(), "{other:?}");
    }

    #[test]
    fn a_rule_lowering_critical_urgency_is_held_by_do_not_disturb() {
        let mut h = with_rules([Rule {
            urgency: Some(Urgency::Low),
            ..rule("battery-nag", "")
        }]);
        h.send(Event::DndSet(DndChange::On));

        let effects = h.notify(
            notification(1)
                .app("battery-nag")
                .urgency(Urgency::Critical),
        );

        assert!(shown(&effects).is_empty(), "{effects:?}");
    }

    #[test]
    fn a_rule_can_keep_a_notification_out_of_history() {
        let mut h = with_rules([Rule {
            history: false,
            ..rule("spotify", "")
        }]);

        let effects = h.notify(notification(1).app("spotify"));

        assert_eq!(shown(&effects), [1]);
        assert_eq!(history_count(&effects), None);
        assert!(h.engine.history().next().is_none());
    }

    #[test]
    fn a_hidden_notification_kept_out_of_history_leaves_no_trace() {
        let mut h = with_rules([Rule {
            history: false,
            ..hiding("discord", "*typing*")
        }]);

        let effects = h.notify(notification(1).app("discord").summary("typing"));

        assert!(effects.is_empty(), "{effects:?}");
    }

    #[test]
    fn a_replaced_config_brings_new_rules() {
        let mut h = Harness::new();
        h.send(Event::ConfigReplaced(EngineConfig {
            rules: Rules(vec![hiding("discord", "")]),
            ..Default::default()
        }));

        assert!(shown(&h.notify(notification(1).app("discord"))).is_empty());
    }
}
