//! Do Not Disturb: while it is on, incoming Notifications go to History
//! without a Popup, except Critical ones when
//! [`DndConfig::allow_critical`] is set. It always starts off, and Popups
//! already on screen are left alone when it is switched on.

use super::{DndChange, Effect, Engine, Notification, Urgency};

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
pub(super) struct Dnd {
    on: bool,
}

impl Dnd {
    pub fn is_on(&self) -> bool {
        self.on
    }
}

impl Engine {
    /// Whether Do Not Disturb lets this Notification show a Popup. A
    /// Notification that replaces a visible Popup still updates it in place.
    pub(super) fn dnd_allows(&self, notification: &Notification) -> bool {
        !self.dnd.on
            || (self.config.dnd.allow_critical && notification.hints.urgency == Urgency::Critical)
            || self.stack.is_visible(notification.id)
    }

    /// Switch Do Not Disturb, reporting the new status if it changed.
    pub(super) fn set_dnd(&mut self, change: DndChange, out: &mut Vec<Effect>) {
        let on = match change {
            DndChange::On => true,
            DndChange::Off => false,
            DndChange::Toggle => !self.dnd.on,
        };
        if on != self.dnd.on {
            self.dnd.on = on;
            out.push(Effect::StatusChanged(self.status()));
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::engine::testing::*;
    use crate::engine::{
        CloseReason, DndChange, DndConfig, Effect, EngineConfig, Event, Status, Urgency,
    };

    /// The last Do Not Disturb state reported in a status change, if any.
    fn dnd_status(effects: &[Effect]) -> Option<bool> {
        effects.iter().rev().find_map(|e| match e {
            Effect::StatusChanged(status) => Some(status.dnd),
            _ => None,
        })
    }

    fn with_allow_critical(allow_critical: bool) -> Harness {
        Harness::with_config(EngineConfig {
            dnd: DndConfig { allow_critical },
            ..Default::default()
        })
    }

    #[test]
    fn do_not_disturb_starts_off() {
        let mut h = Harness::new();

        assert!(!h.engine.status().dnd);
        assert_eq!(shown(&h.notify(notification(1))), [1]);
    }

    #[test]
    fn turning_do_not_disturb_on_and_off_reports_the_status() {
        let mut h = Harness::new();
        h.notify(notification(1));

        let on = h.send(Event::DndSet(DndChange::On));
        let off = h.send(Event::DndSet(DndChange::Off));

        assert_eq!(
            on,
            [Effect::StatusChanged(Status {
                history_count: 1,
                dnd: true
            })]
        );
        assert_eq!(
            off,
            [Effect::StatusChanged(Status {
                history_count: 1,
                dnd: false
            })]
        );
    }

    #[test]
    fn toggling_flips_do_not_disturb() {
        let mut h = Harness::new();

        let first = h.send(Event::DndSet(DndChange::Toggle));
        let second = h.send(Event::DndSet(DndChange::Toggle));

        assert_eq!(dnd_status(&first), Some(true));
        assert_eq!(dnd_status(&second), Some(false));
    }

    #[test]
    fn setting_do_not_disturb_to_its_current_state_changes_nothing() {
        let mut h = Harness::new();
        assert!(h.send(Event::DndSet(DndChange::Off)).is_empty());

        h.send(Event::DndSet(DndChange::On));
        assert!(h.send(Event::DndSet(DndChange::On)).is_empty());
    }

    #[test]
    fn do_not_disturb_holds_notifications_in_history_without_a_popup() {
        let mut h = Harness::new();
        h.send(Event::DndSet(DndChange::On));

        let low = h.notify(notification(1).urgency(Urgency::Low));
        let normal = h.notify(notification(2));

        assert!(shown(&low).is_empty(), "{low:?}");
        assert!(shown(&normal).is_empty(), "{normal:?}");
        assert_eq!(history_count(&normal), Some(2));
        assert_eq!(dnd_status(&normal), Some(true));
        assert!(h.pending_timers().is_empty());
    }

    #[test]
    fn restoring_from_history_shows_a_popup_even_during_do_not_disturb() {
        let mut h = Harness::new();
        h.send(Event::DndSet(DndChange::On));
        h.notify(notification(1));

        let effects = h.send(Event::HistoryRestored(Some(1)));

        assert_eq!(shown(&effects).len(), 1, "{effects:?}");
    }

    #[test]
    fn critical_notifications_pass_through_do_not_disturb_by_default() {
        let mut h = Harness::new();
        h.send(Event::DndSet(DndChange::On));

        let effects = h.notify(notification(1).urgency(Urgency::Critical));

        assert_eq!(shown(&effects), [1]);
        assert_eq!(history_count(&effects), Some(1));
    }

    #[test]
    fn critical_notifications_are_held_too_when_allow_critical_is_off() {
        let mut h = with_allow_critical(false);
        h.send(Event::DndSet(DndChange::On));

        let effects = h.notify(notification(1).urgency(Urgency::Critical));

        assert!(shown(&effects).is_empty(), "{effects:?}");
        assert_eq!(history_count(&effects), Some(1));
    }

    #[test]
    fn allow_critical_off_changes_nothing_while_do_not_disturb_is_off() {
        let mut h = with_allow_critical(false);

        let effects = h.notify(notification(1).urgency(Urgency::Critical));

        assert_eq!(shown(&effects), [1]);
    }

    #[test]
    fn turning_do_not_disturb_off_shows_new_notifications_again() {
        let mut h = Harness::new();
        h.send(Event::DndSet(DndChange::On));
        h.notify(notification(1));
        h.send(Event::DndSet(DndChange::Off));

        let effects = h.notify(notification(2));

        assert_eq!(shown(&effects), [2]);
    }

    #[test]
    fn turning_do_not_disturb_on_leaves_visible_popups_alone() {
        let mut h = Harness::new();
        h.notify(notification(1));

        let effects = h.send(Event::DndSet(DndChange::On));

        assert!(removed(&effects).is_empty(), "{effects:?}");
        assert_eq!(closed(&h.advance(secs(5))), [(1, CloseReason::Expired)]);
    }

    #[test]
    fn a_visible_popup_still_updates_in_place_during_do_not_disturb() {
        let mut h = Harness::new();
        h.notify(notification(1).summary("Downloading"));
        h.send(Event::DndSet(DndChange::On));

        let effects = h.notify(notification(1).summary("Done"));

        assert_eq!(updated(&effects), [1]);
    }

    #[test]
    fn a_config_reload_keeps_do_not_disturb_on() {
        let mut h = Harness::new();
        h.send(Event::DndSet(DndChange::On));

        h.send(Event::ConfigReplaced(EngineConfig::default()));

        assert!(h.engine.status().dnd);
        assert!(shown(&h.notify(notification(1))).is_empty());
    }
}
