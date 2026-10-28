//! The visible Popups and the Overflow waiting behind them.

use std::collections::VecDeque;

use super::{CloseReason, Effect, Engine, Notification, NotificationId, Popup};

/// Visible Popups (oldest first) and Overflow (first in, first out).
#[derive(Debug, Default)]
pub(super) struct Stack {
    visible: Vec<Popup>,
    overflow: VecDeque<Notification>,
}

impl Stack {
    pub fn is_visible(&self, id: NotificationId) -> bool {
        self.visible.iter().any(|p| p.id() == id)
    }

    /// Ids of the visible Popups, oldest first.
    pub fn visible_ids(&self) -> Vec<NotificationId> {
        self.visible.iter().map(Popup::id).collect()
    }

    /// The Notification behind a visible Popup.
    pub fn visible(&self, id: NotificationId) -> Option<&Notification> {
        self.visible
            .iter()
            .find(|p| p.id() == id)
            .map(|p| &p.notification)
    }

    pub fn remove_visible(&mut self, id: NotificationId) -> Option<Popup> {
        let index = self.visible.iter().position(|p| p.id() == id)?;
        Some(self.visible.remove(index))
    }

    pub fn overflow_len(&self) -> usize {
        self.overflow.len()
    }

    /// Replace a waiting Notification with the same id. Gives the
    /// Notification back if none is waiting.
    pub fn replace_in_overflow(&mut self, notification: Notification) -> Option<Notification> {
        match self.overflow.iter_mut().find(|n| n.id == notification.id) {
            Some(waiting) => {
                *waiting = notification;
                None
            }
            None => Some(notification),
        }
    }

    pub fn remove_from_overflow(&mut self, id: NotificationId) -> bool {
        let before = self.overflow.len();
        self.overflow.retain(|n| n.id != id);
        self.overflow.len() != before
    }

    pub fn take_overflow(&mut self) -> Vec<Notification> {
        self.overflow.drain(..).collect()
    }
}

impl Engine {
    /// The most recently shown Popup still on screen (`lucent dismiss`).
    pub fn newest_popup(&self) -> Option<NotificationId> {
        self.stack.visible.last().map(Popup::id)
    }

    /// Put a Notification on screen: update its Popup in place if it is
    /// visible, update it in Overflow if it is waiting there, otherwise
    /// show it or queue it in Overflow when the stack is full.
    pub(super) fn present(&mut self, notification: Notification, out: &mut Vec<Effect>) {
        if self.stack.is_visible(notification.id) {
            let popup = self.popup_for(notification);
            if let Some(slot) = self.stack.visible.iter_mut().find(|p| p.id() == popup.id()) {
                *slot = popup.clone();
            }
            out.push(Effect::UpdatePopup(popup.clone()));
            self.start_expiry(&popup, out);
            return;
        }
        let Some(notification) = self.stack.replace_in_overflow(notification) else {
            return;
        };
        if self.stack.visible.len() < self.config.max_visible {
            self.show(notification, out);
        } else {
            self.stack.overflow.push_back(notification);
            out.push(Effect::OverflowChanged {
                count: self.stack.overflow_len(),
            });
        }
    }

    /// Show waiting Notifications while there is room on screen.
    pub(super) fn fill_from_overflow(&mut self, out: &mut Vec<Effect>) {
        let mut promoted = false;
        while self.stack.visible.len() < self.config.max_visible {
            let Some(notification) = self.stack.overflow.pop_front() else {
                break;
            };
            self.show(notification, out);
            promoted = true;
        }
        if promoted {
            out.push(Effect::OverflowChanged {
                count: self.stack.overflow_len(),
            });
        }
    }

    /// The "+N more" pill was clicked: dismiss the oldest visible Popup so
    /// the next one in Overflow comes in. A click that arrives after
    /// Overflow emptied (the pill was on its way out) does nothing.
    pub(super) fn advance_overflow(&mut self, out: &mut Vec<Effect>) {
        if self.stack.overflow_len() == 0 {
            return;
        }
        if let Some(oldest) = self.stack.visible.first().map(Popup::id) {
            self.close(oldest, CloseReason::Dismissed, out);
        }
    }

    fn show(&mut self, notification: Notification, out: &mut Vec<Effect>) {
        let popup = self.popup_for(notification);
        self.stack.visible.push(popup.clone());
        out.push(Effect::ShowPopup(popup.clone()));
        self.start_expiry(&popup, out);
    }

    fn popup_for(&self, notification: Notification) -> Popup {
        Popup {
            timeout: self.timeout_for(&notification),
            received_at: self.clock.now(),
            notification,
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::engine::testing::*;
    use crate::engine::{CloseReason, EngineConfig, Event};

    fn two_visible() -> Harness {
        Harness::with_config(EngineConfig {
            max_visible: 2,
            ..Default::default()
        })
    }

    #[test]
    fn the_newest_popup_is_the_last_one_shown_that_is_still_open() {
        let mut h = two_visible();
        assert_eq!(h.engine.newest_popup(), None);
        for id in 1..=3 {
            h.notify(notification(id));
        }
        assert_eq!(h.engine.newest_popup(), Some(2), "3 waits in Overflow");

        h.send(Event::UserDismissed(2));

        assert_eq!(h.engine.newest_popup(), Some(3));
    }

    #[test]
    fn extras_wait_in_overflow_when_the_stack_is_full() {
        let mut h = two_visible();
        h.notify(notification(1));
        h.notify(notification(2));

        let effects = h.notify(notification(3));

        assert!(shown(&effects).is_empty());
        assert_eq!(overflow_count(&effects), Some(1));
    }

    #[test]
    fn closing_a_popup_shows_the_next_in_overflow_first_in_first_out() {
        let mut h = two_visible();
        for id in 1..=4 {
            h.notify(notification(id));
        }

        let first = h.send(Event::UserDismissed(1));
        let second = h.send(Event::AppClosed(2));

        assert_eq!(shown(&first), [3]);
        assert_eq!(overflow_count(&first), Some(1));
        assert_eq!(shown(&second), [4]);
        assert_eq!(overflow_count(&second), Some(0));
    }

    #[test]
    fn the_app_closing_a_waiting_notification_takes_it_out_of_overflow() {
        let mut h = two_visible();
        for id in 1..=3 {
            h.notify(notification(id));
        }

        let effects = h.send(Event::AppClosed(3));

        assert_eq!(closed(&effects), [(3, CloseReason::ClosedByApp)]);
        assert!(removed(&effects).is_empty(), "it was never on screen");
        assert_eq!(overflow_count(&effects), Some(0));
        assert!(shown(&h.send(Event::UserDismissed(1))).is_empty());
    }

    #[test]
    fn a_waiting_notification_starts_its_timeout_only_once_shown() {
        let mut h = Harness::with_config(EngineConfig {
            max_visible: 1,
            ..Default::default()
        });
        h.notify(notification(1));
        h.advance(secs(3));
        h.notify(notification(2));

        let at_5s = h.advance(secs(2));
        let at_9s = h.advance(secs(4));
        let at_10s = h.advance(secs(1));

        assert_eq!(closed(&at_5s), [(1, CloseReason::Expired)]);
        assert_eq!(shown(&at_5s), [2]);
        assert!(at_9s.is_empty(), "{at_9s:?}");
        assert_eq!(closed(&at_10s), [(2, CloseReason::Expired)]);
    }

    #[test]
    fn a_burst_beyond_max_visible_waits_behind_the_pill() {
        let mut h = Harness::new();
        let mut counts = Vec::new();
        let mut shown_ids = Vec::new();
        for id in 1..=8 {
            let effects = h.notify(notification(id));
            shown_ids.extend(shown(&effects));
            counts.extend(overflow_count(&effects));
        }

        assert_eq!(shown_ids, [1, 2, 3, 4, 5]);
        assert_eq!(counts, [1, 2, 3], "+3 more");
    }

    #[test]
    fn clicking_the_pill_dismisses_the_oldest_popup_and_shows_the_next() {
        let mut h = two_visible();
        for id in 1..=4 {
            h.notify(notification(id));
        }

        let first = h.send(Event::OverflowAdvanced);
        let second = h.send(Event::OverflowAdvanced);

        assert_eq!(removed(&first), [(1, CloseReason::Dismissed)]);
        assert_eq!(closed(&first), [(1, CloseReason::Dismissed)]);
        assert_eq!(shown(&first), [3]);
        assert_eq!(overflow_count(&first), Some(1));
        assert_eq!(removed(&second), [(2, CloseReason::Dismissed)]);
        assert_eq!(shown(&second), [4]);
        assert_eq!(overflow_count(&second), Some(0));
    }

    #[test]
    fn the_pill_advances_past_popups_closed_out_of_order() {
        let mut h = two_visible();
        for id in 1..=4 {
            h.notify(notification(id));
        }
        h.send(Event::UserDismissed(1));

        let effects = h.send(Event::OverflowAdvanced);

        assert_eq!(removed(&effects), [(2, CloseReason::Dismissed)]);
        assert_eq!(shown(&effects), [4]);
    }

    #[test]
    fn a_stale_pill_click_with_nothing_waiting_closes_nothing() {
        let mut h = two_visible();
        h.notify(notification(1));
        h.notify(notification(2));

        assert!(h.send(Event::OverflowAdvanced).is_empty());
    }

    #[test]
    fn raising_max_visible_brings_waiting_notifications_on_screen() {
        let mut h = two_visible();
        for id in 1..=5 {
            h.notify(notification(id));
        }

        let effects = h.send(Event::ConfigReplaced(EngineConfig {
            max_visible: 4,
            ..Default::default()
        }));

        assert_eq!(shown(&effects), [3, 4]);
        assert_eq!(overflow_count(&effects), Some(1));
    }

    #[test]
    fn replacing_a_waiting_notification_updates_it_in_overflow() {
        let mut h = two_visible();
        h.notify(notification(1));
        h.notify(notification(2));
        h.notify(notification(3).summary("old"));

        let replaced = h.notify(notification(3).summary("new"));
        let promoted = h.send(Event::UserDismissed(1));

        assert!(shown(&replaced).is_empty());
        assert!(updated(&replaced).is_empty());
        let popup = promoted
            .iter()
            .find_map(|e| match e {
                crate::engine::Effect::ShowPopup(p) => Some(p.clone()),
                _ => None,
            })
            .expect("3 should be shown");
        assert_eq!(popup.notification.summary, "new");
    }
}
