//! Receiving, replacing and closing Notifications.

use super::{CloseReason, Effect, Engine, Notification, NotificationId};

impl Engine {
    /// The receive pipeline: Rules first, then Do Not Disturb and the stack,
    /// then History. The Popup's effects come before History's status
    /// change, so the Popup appears first.
    pub(super) fn receive(&mut self, mut notification: Notification, out: &mut Vec<Effect>) {
        let outcome = self.apply_rules(&mut notification);
        if outcome.show && self.dnd_allows(&notification) {
            self.present(notification.clone(), out);
        }
        if outcome.record {
            self.record_in_history(&notification, out);
        }
    }

    /// Close a Notification wherever it is. Unknown ids are ignored, so
    /// `NotificationClosed` is emitted at most once per Notification.
    pub(super) fn close(&mut self, id: NotificationId, reason: CloseReason, out: &mut Vec<Effect>) {
        if self.stack.remove_visible(id).is_some() {
            self.stop_expiry(id, out);
            out.push(Effect::RemovePopup { id, reason });
            out.push(Effect::NotificationClosed { id, reason });
            self.fill_from_overflow(out);
        } else if self.stack.remove_from_overflow(id) {
            out.push(Effect::NotificationClosed { id, reason });
            out.push(Effect::OverflowChanged {
                count: self.stack.overflow_len(),
            });
        }
    }

    /// Close everything waiting in Overflow, then every Popup.
    pub(super) fn dismiss_all(&mut self, out: &mut Vec<Effect>) {
        let waiting = self.stack.take_overflow();
        if !waiting.is_empty() {
            for notification in waiting {
                out.push(Effect::NotificationClosed {
                    id: notification.id,
                    reason: CloseReason::Dismissed,
                });
            }
            out.push(Effect::OverflowChanged { count: 0 });
        }
        for id in self.stack.visible_ids() {
            self.close(id, CloseReason::Dismissed, out);
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::engine::testing::*;
    use crate::engine::{CloseReason, Effect, Event};

    #[test]
    fn a_notification_shows_a_popup() {
        let mut h = Harness::new();
        let n = notification(1)
            .app("chat")
            .summary("hi")
            .body("there")
            .build();

        let effects = h.notify(n.clone());

        let Some(Effect::ShowPopup(popup)) = effects.first() else {
            panic!("expected a Popup first, got {effects:?}");
        };
        assert_eq!(popup.notification, n);
        assert_eq!(shown(&effects), [1]);
        assert!(closed(&effects).is_empty());
    }

    #[test]
    fn the_app_closing_a_popup_removes_it_with_reason_3() {
        let mut h = Harness::new();
        h.notify(notification(1));

        let effects = h.send(Event::AppClosed(1));

        assert_eq!(removed(&effects), [(1, CloseReason::ClosedByApp)]);
        assert_eq!(closed(&effects), [(1, CloseReason::ClosedByApp)]);
        assert_eq!(CloseReason::ClosedByApp.code(), 3);
    }

    #[test]
    fn notification_closed_is_emitted_exactly_once() {
        let mut h = Harness::new();
        h.notify(notification(1));

        let first = h.send(Event::AppClosed(1));
        let again = h.send(Event::AppClosed(1));
        let dismissed_after = h.send(Event::UserDismissed(1));
        let later = h.advance(secs(60));

        assert_eq!(closed(&first).len(), 1);
        assert!(again.is_empty(), "{again:?}");
        assert!(dismissed_after.is_empty(), "{dismissed_after:?}");
        assert!(later.is_empty(), "{later:?}");
    }

    #[test]
    fn closing_an_unknown_id_does_nothing() {
        let mut h = Harness::new();
        h.notify(notification(1));

        assert!(h.send(Event::AppClosed(42)).is_empty());
        assert!(h.send(Event::UserDismissed(42)).is_empty());
    }

    #[test]
    fn the_user_dismissing_a_popup_closes_it_with_reason_2() {
        let mut h = Harness::new();
        h.notify(notification(1));

        let effects = h.send(Event::UserDismissed(1));

        assert_eq!(removed(&effects), [(1, CloseReason::Dismissed)]);
        assert_eq!(closed(&effects), [(1, CloseReason::Dismissed)]);
        assert_eq!(CloseReason::Dismissed.code(), 2);
        assert!(h.pending_timers().is_empty());
        assert!(h.advance(secs(60)).is_empty());
    }

    #[test]
    fn a_notification_with_a_known_id_replaces_the_visible_popup_in_place() {
        let mut h = Harness::new();
        h.notify(notification(1).summary("Downloading"));

        let effects = h.notify(notification(1).summary("Done"));

        assert_eq!(updated(&effects), [1]);
        assert!(shown(&effects).is_empty());
        assert!(closed(&effects).is_empty());
        let Some(Effect::UpdatePopup(popup)) = effects.first() else {
            panic!("expected an update, got {effects:?}");
        };
        assert_eq!(popup.notification.summary, "Done");
    }

    #[test]
    fn dismissing_all_closes_every_popup_and_overflow() {
        let mut h = Harness::with_config(crate::engine::EngineConfig {
            max_visible: 1,
            ..Default::default()
        });
        h.notify(notification(1));
        h.notify(notification(2));

        let effects = h.send(Event::UserDismissedAll);

        let mut closed = closed(&effects);
        closed.sort_by_key(|(id, _)| *id);
        assert_eq!(
            closed,
            [(1, CloseReason::Dismissed), (2, CloseReason::Dismissed)]
        );
        assert_eq!(removed(&effects), [(1, CloseReason::Dismissed)]);
        assert!(shown(&effects).is_empty(), "nothing should be promoted");
        assert_eq!(overflow_count(&effects), Some(0));
        assert!(h.send(Event::UserDismissedAll).is_empty());
    }
}
