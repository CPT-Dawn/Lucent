//! Actions and click semantics.

use super::{CloseReason, Effect, Engine, MouseButton, NotificationId};

impl Engine {
    /// The user invoked an Action on a Popup: tell the app, then close the
    /// Popup as dismissed. Only Actions a visible Popup offers count, so a
    /// late click never reaches an app that already heard the Popup close.
    pub(super) fn invoke_action(&mut self, id: NotificationId, key: String, out: &mut Vec<Effect>) {
        let offered = self
            .stack
            .visible(id)
            .is_some_and(|n| n.actions.iter().any(|a| a.key == key));
        if !offered {
            return;
        }
        out.push(Effect::ActionInvoked { id, key });
        self.close(id, CloseReason::Dismissed, out);
    }

    /// The user clicked a Popup's body: left invokes the default Action
    /// (or dismisses when there is none), middle dismisses every Popup,
    /// right dismisses this one. Clicks on a Popup that is already gone
    /// do nothing.
    pub(super) fn popup_clicked(
        &mut self,
        id: NotificationId,
        button: MouseButton,
        out: &mut Vec<Effect>,
    ) {
        let Some(notification) = self.stack.visible(id) else {
            return;
        };
        match button {
            MouseButton::Primary if notification.actions.iter().any(|a| a.key == DEFAULT) => {
                self.invoke_action(id, DEFAULT.into(), out)
            }
            MouseButton::Primary | MouseButton::Secondary => {
                self.close(id, CloseReason::Dismissed, out)
            }
            MouseButton::Middle => self.dismiss_all(out),
        }
    }
}

/// The Action a body click invokes.
const DEFAULT: &str = "default";

#[cfg(test)]
mod tests {
    use crate::engine::testing::*;
    use crate::engine::{CloseReason, Effect, Event, MouseButton, NotificationId};

    fn invoke(id: NotificationId, key: &str) -> Event {
        Event::ActionInvoked {
            id,
            key: key.into(),
        }
    }

    #[test]
    fn invoking_an_action_tells_the_app_then_closes_the_popup_as_dismissed() {
        let mut h = Harness::new();
        h.notify(
            notification(1)
                .action("reply", "Reply")
                .action("read", "Mark read"),
        );

        let effects = h.send(invoke(1, "read"));

        let invoked = effects
            .iter()
            .position(|e| matches!(e, Effect::ActionInvoked { id: 1, key } if key == "read"))
            .expect("ActionInvoked(1, \"read\")");
        let closed_at = effects
            .iter()
            .position(|e| matches!(e, Effect::NotificationClosed { .. }))
            .expect("NotificationClosed");
        assert!(
            invoked < closed_at,
            "the app hears the Action first: {effects:?}"
        );
        assert_eq!(removed(&effects), [(1, CloseReason::Dismissed)]);
        assert_eq!(closed(&effects), [(1, CloseReason::Dismissed)]);
        assert!(h.pending_timers().is_empty());
    }

    fn action_invoked(effects: &[Effect]) -> Vec<(NotificationId, String)> {
        effects
            .iter()
            .filter_map(|e| match e {
                Effect::ActionInvoked { id, key } => Some((*id, key.clone())),
                _ => None,
            })
            .collect()
    }

    fn click(id: NotificationId, button: MouseButton) -> Event {
        Event::PopupClicked { id, button }
    }

    #[test]
    fn a_left_click_invokes_the_default_action_then_closes_the_popup() {
        let mut h = Harness::new();
        h.notify(
            notification(1)
                .action("reply", "Reply")
                .action("default", "Open"),
        );

        let effects = h.send(click(1, MouseButton::Primary));

        assert_eq!(action_invoked(&effects), [(1, "default".to_string())]);
        assert_eq!(closed(&effects), [(1, CloseReason::Dismissed)]);
    }

    #[test]
    fn a_left_click_without_a_default_action_only_dismisses() {
        let mut h = Harness::new();
        h.notify(notification(1).action("reply", "Reply"));

        let effects = h.send(click(1, MouseButton::Primary));

        assert!(action_invoked(&effects).is_empty(), "{effects:?}");
        assert_eq!(closed(&effects), [(1, CloseReason::Dismissed)]);
    }

    #[test]
    fn a_right_click_dismisses_without_invoking_the_default_action() {
        let mut h = Harness::new();
        h.notify(notification(1).action("default", "Open"));
        h.notify(notification(2));

        let effects = h.send(click(1, MouseButton::Secondary));

        assert!(action_invoked(&effects).is_empty(), "{effects:?}");
        assert_eq!(closed(&effects), [(1, CloseReason::Dismissed)]);
    }

    #[test]
    fn a_middle_click_dismisses_every_popup_without_invoking_anything() {
        let mut h = Harness::new();
        h.notify(notification(1).action("default", "Open"));
        h.notify(notification(2).action("reply", "Reply"));

        let effects = h.send(click(2, MouseButton::Middle));

        assert!(action_invoked(&effects).is_empty(), "{effects:?}");
        let mut closed = closed(&effects);
        closed.sort_by_key(|(id, _)| *id);
        assert_eq!(
            closed,
            [(1, CloseReason::Dismissed), (2, CloseReason::Dismissed)]
        );
    }

    #[test]
    fn a_click_on_a_popup_that_is_already_gone_does_nothing() {
        let mut h = Harness::new();
        h.notify(notification(1).action("default", "Open"));
        h.notify(notification(2));
        h.send(Event::UserDismissed(1));

        for button in [
            MouseButton::Primary,
            MouseButton::Middle,
            MouseButton::Secondary,
        ] {
            assert!(h.send(click(1, button)).is_empty(), "{button:?}");
            assert!(h.send(click(42, button)).is_empty(), "{button:?}");
        }
    }

    #[test]
    fn invoking_an_action_makes_room_for_the_next_in_overflow() {
        let mut h = Harness::with_config(crate::engine::EngineConfig {
            max_visible: 1,
            ..Default::default()
        });
        h.notify(notification(1).action("reply", "Reply"));
        h.notify(notification(2));

        let effects = h.send(invoke(1, "reply"));

        assert_eq!(shown(&effects), [2]);
        assert_eq!(overflow_count(&effects), Some(0));
    }

    #[test]
    fn an_action_on_a_popup_that_is_already_gone_does_nothing() {
        let mut h = Harness::new();
        h.notify(notification(1).action("reply", "Reply"));
        h.send(Event::UserDismissed(1));

        assert!(h.send(invoke(1, "reply")).is_empty());
        assert!(h.send(invoke(42, "reply")).is_empty());
    }

    #[test]
    fn an_action_the_notification_does_not_offer_does_nothing() {
        let mut h = Harness::new();
        h.notify(notification(1).action("reply", "Reply"));

        assert!(h.send(invoke(1, "default")).is_empty());
        assert!(h.send(invoke(1, "delete-everything")).is_empty());
    }
}
