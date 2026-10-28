//! History: Lucent's in-memory record of received Notifications.
//!
//! Every non-Transient Notification is recorded, whether or not it gets a
//! Popup and however it closes. A Notification that replaces another keeps
//! that single entry, in its latest state. History is capped at
//! [`HistoryConfig::limit`] entries and the oldest drop first. It is never
//! written to disk.

use std::collections::VecDeque;
use std::time::Instant;

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

/// One remembered Notification.
#[derive(Debug, Clone, PartialEq)]
pub struct HistoryEntry {
    /// The Notification in its latest state, after Rules.
    pub notification: Notification,
    /// When the engine received (or last replaced) it.
    pub received_at: Instant,
}

/// History entries, newest first.
#[derive(Debug, Default)]
pub(super) struct History {
    entries: VecDeque<HistoryEntry>,
}

impl History {
    pub fn len(&self) -> usize {
        self.entries.len()
    }
}

impl Engine {
    /// History, newest first.
    pub fn history(&self) -> impl Iterator<Item = &HistoryEntry> {
        self.history.entries.iter()
    }

    /// Called for every received Notification that a Rule doesn't keep out
    /// of History, whether or not it gets a Popup.
    pub(super) fn record_in_history(&mut self, notification: &Notification, out: &mut Vec<Effect>) {
        if notification.hints.transient {
            return;
        }
        let before = self.history.len();
        let entry = HistoryEntry {
            notification: notification.clone(),
            received_at: self.clock.now(),
        };
        let entries = &mut self.history.entries;
        match entries
            .iter_mut()
            .find(|e| e.notification.id == entry.notification.id)
        {
            Some(existing) => *existing = entry,
            None => entries.push_front(entry),
        }
        self.history.entries.truncate(self.config.history.limit);
        self.report_history_count(before, out);
    }

    /// Drop the oldest entries beyond the configured limit, which may just
    /// have shrunk.
    pub(super) fn apply_history_limit(&mut self, out: &mut Vec<Effect>) {
        let before = self.history.len();
        self.history.entries.truncate(self.config.history.limit);
        self.report_history_count(before, out);
    }

    /// Push a status change if the History count is no longer `before`.
    fn report_history_count(&self, before: usize, out: &mut Vec<Effect>) {
        if self.history.len() != before {
            out.push(Effect::StatusChanged(self.status()));
        }
    }

    /// Show a History entry (the most recent one if `id` is `None`) as a
    /// new Popup with a fresh id. History itself is left as it was. The
    /// app has long forgotten the original, so its Actions are dropped.
    /// Unknown ids are ignored.
    pub(super) fn restore_from_history(
        &mut self,
        id: Option<NotificationId>,
        out: &mut Vec<Effect>,
    ) {
        let mut entries = self.history.entries.iter();
        let entry = match id {
            None => entries.next(),
            Some(id) => entries.find(|e| e.notification.id == id),
        };
        let Some(entry) = entry else {
            return;
        };
        let restored = Notification {
            id: self.ids.next(),
            actions: Vec::new(),
            ..entry.notification.clone()
        };
        self.present(restored, out);
    }

    /// Forget every entry. Popups on screen stay.
    pub(super) fn clear_history(&mut self, out: &mut Vec<Effect>) {
        let before = self.history.len();
        self.history.entries.clear();
        self.report_history_count(before, out);
    }
}

#[cfg(test)]
mod tests {
    use crate::engine::testing::*;
    use crate::engine::{Clock, CloseReason, Effect, EngineConfig, Event, HistoryConfig, Popup};

    /// History's ids, newest first.
    fn history_ids(h: &Harness) -> Vec<u32> {
        h.engine.history().map(|e| e.notification.id).collect()
    }

    #[test]
    fn received_notifications_are_listed_newest_first() {
        let mut h = Harness::new();
        h.notify(notification(1).summary("first"));
        h.advance(secs(1));
        h.notify(notification(2).summary("second"));

        assert_eq!(history_ids(&h), [2, 1]);
        let newest = h.engine.history().next().unwrap();
        assert_eq!(newest.notification.summary, "second");
        assert_eq!(newest.received_at, h.clock.now());
    }

    #[test]
    fn transient_notifications_show_but_are_never_recorded() {
        let mut h = Harness::new();
        h.notify(notification(1));

        let effects = h.notify(notification(2).summary("Volume 40%").transient());

        assert_eq!(shown(&effects), [2]);
        assert_eq!(history_ids(&h), [1]);
    }

    #[test]
    fn notifications_closed_by_the_app_or_the_user_stay_in_history() {
        let mut h = Harness::new();
        h.notify(notification(1));
        h.notify(notification(2));
        h.notify(notification(3));

        h.send(Event::AppClosed(1));
        h.send(Event::UserDismissed(2));
        h.advance(secs(60));

        assert_eq!(history_ids(&h), [3, 2, 1]);
    }

    #[test]
    fn a_replaced_notification_keeps_one_entry_in_its_latest_state_and_place() {
        let mut h = Harness::new();
        h.notify(notification(1).summary("Downloading").progress(10));
        h.notify(notification(2));
        h.advance(secs(2));

        h.notify(notification(1).summary("Downloaded").progress(100));

        assert_eq!(history_ids(&h), [2, 1]);
        let entry = h.engine.history().nth(1).unwrap();
        assert_eq!(entry.notification.summary, "Downloaded");
        assert_eq!(entry.notification.hints.progress, Some(100));
        assert_eq!(entry.received_at, h.clock.now());
    }

    #[test]
    fn a_replacement_also_updates_an_entry_whose_popup_is_gone() {
        let mut h = Harness::new();
        h.notify(notification(1).summary("old"));
        h.send(Event::UserDismissed(1));

        h.notify(notification(1).summary("new"));

        assert_eq!(history_ids(&h), [1]);
        assert_eq!(
            h.engine.history().next().unwrap().notification.summary,
            "new"
        );
    }

    fn limited_to(limit: usize) -> Harness {
        Harness::with_config(EngineConfig {
            history: HistoryConfig { limit },
            ..Default::default()
        })
    }

    #[test]
    fn history_drops_its_oldest_entries_beyond_the_limit() {
        let mut h = limited_to(2);
        for id in 1..=4 {
            h.notify(notification(id));
        }

        assert_eq!(history_ids(&h), [4, 3]);
    }

    #[test]
    fn a_smaller_limit_in_a_new_config_trims_the_oldest_entries() {
        let mut h = Harness::new();
        for id in 1..=3 {
            h.notify(notification(id));
        }

        let effects = h.send(Event::ConfigReplaced(EngineConfig {
            history: HistoryConfig { limit: 1 },
            ..Default::default()
        }));

        assert_eq!(history_ids(&h), [3]);
        assert_eq!(history_count(&effects), Some(1));
    }

    #[test]
    fn the_status_follows_the_history_count() {
        let mut h = limited_to(2);

        let first = h.notify(notification(1));
        let second = h.notify(notification(2));
        let replaced = h.notify(notification(2).summary("again"));
        let transient = h.notify(notification(3).transient());
        let evicting = h.notify(notification(4));

        assert_eq!(history_count(&first), Some(1));
        assert_eq!(history_count(&second), Some(2));
        assert_eq!(history_count(&replaced), None, "the count didn't change");
        assert_eq!(history_count(&transient), None);
        assert_eq!(history_count(&evicting), None, "still full");
        assert_eq!(h.engine.status().history_count, 2);
    }

    fn the_popup_in(effects: &[Effect]) -> Popup {
        effects
            .iter()
            .find_map(|e| match e {
                Effect::ShowPopup(popup) => Some(popup.clone()),
                _ => None,
            })
            .unwrap_or_else(|| panic!("expected a Popup, got {effects:?}"))
    }

    #[test]
    fn restoring_shows_the_most_recent_entry_as_a_new_popup() {
        let mut h = Harness::new();
        h.notify(notification(1).summary("older"));
        h.notify(notification(2).summary("newer").body("details"));
        h.send(Event::UserDismissedAll);

        let effects = h.send(Event::HistoryRestored(None));

        let popup = the_popup_in(&effects);
        assert_eq!(popup.notification.summary, "newer");
        assert_eq!(popup.notification.body, "details");
        assert!(![1, 2].contains(&popup.id()), "restored with a fresh id");
        assert_eq!(popup.timeout, Some(secs(5)));
    }

    #[test]
    fn restoring_a_specific_entry_shows_that_one() {
        let mut h = Harness::new();
        h.notify(notification(1).summary("older"));
        h.notify(notification(2).summary("newer"));

        let effects = h.send(Event::HistoryRestored(Some(1)));

        assert_eq!(the_popup_in(&effects).notification.summary, "older");
    }

    #[test]
    fn a_restored_popup_drops_actions_the_app_no_longer_listens_for() {
        let mut h = Harness::new();
        h.notify(
            notification(1)
                .action("default", "Open")
                .action("reply", "Reply"),
        );

        let effects = h.send(Event::HistoryRestored(None));

        assert!(the_popup_in(&effects).notification.actions.is_empty());
    }

    #[test]
    fn restoring_leaves_history_as_it_was() {
        let mut h = Harness::new();
        h.notify(notification(1));
        h.notify(notification(2));

        let effects = h.send(Event::HistoryRestored(Some(1)));
        let restored = the_popup_in(&effects).id();
        h.send(Event::UserDismissed(restored));

        assert_eq!(history_count(&effects), None);
        assert_eq!(history_ids(&h), [2, 1]);
    }

    #[test]
    fn restoring_from_an_empty_history_or_an_unknown_id_does_nothing() {
        let mut h = Harness::new();
        assert!(h.send(Event::HistoryRestored(None)).is_empty());

        h.notify(notification(1));
        assert!(h.send(Event::HistoryRestored(Some(42))).is_empty());
    }

    #[test]
    fn a_restored_popup_expires_and_closes_like_any_other() {
        let mut h = Harness::new();
        h.notify(notification(1));
        h.send(Event::UserDismissed(1));

        let restored = the_popup_in(&h.send(Event::HistoryRestored(None))).id();

        assert_eq!(
            closed(&h.advance(secs(5))),
            [(restored, CloseReason::Expired)]
        );
    }

    #[test]
    fn clearing_empties_history_but_leaves_popups_on_screen() {
        let mut h = Harness::new();
        h.notify(notification(1));
        h.notify(notification(2));

        let effects = h.send(Event::HistoryCleared);

        assert_eq!(effects, [Effect::StatusChanged(h.engine.status())]);
        assert_eq!(history_count(&effects), Some(0));
        assert!(history_ids(&h).is_empty());
        assert!(h.send(Event::HistoryRestored(None)).is_empty());
        assert_eq!(closed(&h.advance(secs(5))).len(), 2);
    }

    #[test]
    fn clearing_an_empty_history_changes_nothing() {
        let mut h = Harness::new();
        assert!(h.send(Event::HistoryCleared).is_empty());
    }
}
