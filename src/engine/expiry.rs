//! Timeouts: how long each Popup stays, its expiry timer and the hover
//! pause.

use std::collections::{HashMap, HashSet};
use std::time::{Duration, Instant};

use super::{CloseReason, Effect, Engine, Expiry, Notification, NotificationId, Popup, Timer};

/// The countdown of each visible Popup that expires, and which Popups are
/// under the pointer.
#[derive(Debug, Default)]
pub(super) struct Deadlines {
    countdowns: HashMap<NotificationId, Countdown>,
    hovered: HashSet<NotificationId>,
}

/// Where a Popup's countdown stands.
#[derive(Debug, Clone, Copy)]
enum Countdown {
    /// Expires at this instant; its expiry timer is pending.
    Running(Instant),
    /// Hovered: this much time is left and no timer is pending.
    Paused(Duration),
}

impl Engine {
    /// How long a Popup for this Notification stays; `None` means never.
    /// The app's own expiry wins; otherwise the Urgency default applies.
    pub(super) fn timeout_for(&self, notification: &Notification) -> Option<Duration> {
        match notification.expiry {
            Expiry::After(after) => Some(after),
            Expiry::Never => None,
            Expiry::Default => self.config.timeouts.for_urgency(notification.hints.urgency),
        }
    }

    /// (Re)start a Popup's countdown from its full timeout. A Popup under
    /// the pointer starts paused.
    pub(super) fn start_expiry(&mut self, popup: &Popup, out: &mut Vec<Effect>) {
        let id = popup.id();
        self.cancel_countdown(id, out);
        let Some(after) = popup.timeout else {
            return;
        };
        if self.expiry.hovered.contains(&id) {
            self.expiry.countdowns.insert(id, Countdown::Paused(after));
            out.push(Effect::CountdownPaused {
                id,
                remaining: after,
            });
            return;
        }
        self.expiry
            .countdowns
            .insert(id, Countdown::Running(self.clock.now() + after));
        out.push(Effect::ScheduleTimer {
            timer: Timer::Expire(id),
            after,
        });
    }

    /// Forget a Popup's countdown and hover; it has left the screen.
    pub(super) fn stop_expiry(&mut self, id: NotificationId, out: &mut Vec<Effect>) {
        self.expiry.hovered.remove(&id);
        self.cancel_countdown(id, out);
    }

    fn cancel_countdown(&mut self, id: NotificationId, out: &mut Vec<Effect>) {
        if let Some(Countdown::Running(_)) = self.expiry.countdowns.remove(&id) {
            out.push(Effect::CancelTimer(Timer::Expire(id)));
        }
    }

    pub(super) fn timer_fired(&mut self, timer: Timer, out: &mut Vec<Effect>) {
        match timer {
            Timer::Expire(id) => {
                let Some(&Countdown::Running(deadline)) = self.expiry.countdowns.get(&id) else {
                    return;
                };
                let now = self.clock.now();
                if now >= deadline {
                    self.expiry.countdowns.remove(&id);
                    self.close(id, CloseReason::Expired, out);
                } else {
                    out.push(Effect::ScheduleTimer {
                        timer,
                        after: deadline - now,
                    });
                }
            }
        }
    }

    /// The pointer entered a Popup: freeze its countdown.
    pub(super) fn hover_started(&mut self, id: NotificationId, out: &mut Vec<Effect>) {
        if !self.stack.is_visible(id) {
            return;
        }
        self.expiry.hovered.insert(id);
        let now = self.clock.now();
        if let Some(countdown) = self.expiry.countdowns.get_mut(&id)
            && let Countdown::Running(deadline) = *countdown
        {
            let remaining = deadline.saturating_duration_since(now);
            *countdown = Countdown::Paused(remaining);
            out.push(Effect::CancelTimer(Timer::Expire(id)));
            out.push(Effect::CountdownPaused { id, remaining });
        }
    }

    /// The pointer left a Popup: carry on from the time that was left.
    pub(super) fn hover_ended(&mut self, id: NotificationId, out: &mut Vec<Effect>) {
        self.expiry.hovered.remove(&id);
        let now = self.clock.now();
        if let Some(countdown) = self.expiry.countdowns.get_mut(&id)
            && let Countdown::Paused(remaining) = *countdown
        {
            *countdown = Countdown::Running(now + remaining);
            out.push(Effect::ScheduleTimer {
                timer: Timer::Expire(id),
                after: remaining,
            });
            out.push(Effect::CountdownResumed { id, remaining });
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::engine::testing::*;
    use crate::engine::{CloseReason, Effect, Event, Timer, Urgency};

    fn expired(id: u32) -> Vec<(u32, CloseReason)> {
        vec![(id, CloseReason::Expired)]
    }

    #[test]
    fn a_normal_notification_expires_after_the_default_5s() {
        let mut h = Harness::new();
        h.notify(notification(1));

        let before = h.advance(millis(4_999));
        let on_time = h.advance(millis(1));

        assert!(before.is_empty(), "{before:?}");
        assert_eq!(removed(&on_time), expired(1));
        assert_eq!(closed(&on_time), expired(1));
        assert_eq!(CloseReason::Expired.code(), 1);
    }

    #[test]
    fn a_low_urgency_notification_expires_after_3s() {
        let mut h = Harness::new();
        h.notify(notification(1).urgency(Urgency::Low));

        assert!(h.advance(millis(2_999)).is_empty());
        assert_eq!(closed(&h.advance(millis(1))), expired(1));
    }

    #[test]
    fn a_critical_notification_never_expires_by_default() {
        let mut h = Harness::new();
        let effects = h.notify(notification(1).urgency(Urgency::Critical));

        assert!(h.pending_timers().is_empty(), "{effects:?}");
        assert!(h.advance(secs(3_600)).is_empty());
    }

    #[test]
    fn the_apps_own_expiry_wins_over_the_urgency_default() {
        let mut h = Harness::new();
        h.notify(notification(1).expires_after(millis(1_500)));
        h.notify(
            notification(2)
                .urgency(Urgency::Critical)
                .expires_after(secs(2)),
        );

        assert_eq!(closed(&h.advance(millis(1_500))), expired(1));
        assert_eq!(closed(&h.advance(millis(500))), expired(2));
    }

    #[test]
    fn an_app_asking_for_no_expiry_is_never_expired() {
        let mut h = Harness::new();
        h.notify(notification(1).urgency(Urgency::Low).never_expires());

        assert!(h.advance(secs(3_600)).is_empty());
    }

    #[test]
    fn the_popup_carries_its_timeout_for_the_countdown() {
        let mut h = Harness::new();
        let effects = h.notify(notification(1));

        let Some(Effect::ShowPopup(popup)) = effects.first() else {
            panic!("{effects:?}");
        };
        assert_eq!(popup.timeout, Some(secs(5)));
    }

    #[test]
    fn replacing_a_visible_notification_restarts_its_timeout() {
        let mut h = Harness::new();
        h.notify(notification(1));
        h.advance(secs(4));

        h.notify(notification(1).summary("again"));

        assert!(h.advance(secs(4)).is_empty());
        assert_eq!(closed(&h.advance(secs(1))), expired(1));
    }

    #[test]
    fn a_timer_firing_early_is_rescheduled_rather_than_expiring() {
        let mut h = Harness::new();
        h.notify(notification(1));
        h.clock.advance(secs(2));

        let effects = h.send(Event::TimerFired(Timer::Expire(1)));

        assert!(closed(&effects).is_empty(), "{effects:?}");
        assert!(effects.contains(&Effect::ScheduleTimer {
            timer: Timer::Expire(1),
            after: secs(3),
        }));
        assert_eq!(closed(&h.advance(secs(3))), expired(1));
    }

    #[test]
    fn hovering_pauses_the_countdown_and_leaving_resumes_the_remaining_time() {
        let mut h = Harness::new();
        h.notify(notification(1));
        h.advance(secs(2));

        h.send(Event::HoverStarted(1));
        let while_hovered = h.advance(secs(60));
        h.send(Event::HoverEnded(1));
        let before = h.advance(millis(2_999));
        let on_time = h.advance(millis(1));

        assert!(while_hovered.is_empty(), "{while_hovered:?}");
        assert!(before.is_empty(), "{before:?}");
        assert_eq!(closed(&on_time), expired(1));
    }

    #[test]
    fn the_countdown_line_learns_when_it_is_paused_and_resumed() {
        let mut h = Harness::new();
        h.notify(notification(1));
        h.advance(millis(1_500));

        let paused = h.send(Event::HoverStarted(1));
        h.advance(secs(10));
        let resumed = h.send(Event::HoverEnded(1));

        assert!(paused.contains(&Effect::CountdownPaused {
            id: 1,
            remaining: millis(3_500),
        }));
        assert!(resumed.contains(&Effect::CountdownResumed {
            id: 1,
            remaining: millis(3_500),
        }));
    }

    #[test]
    fn a_replacement_while_hovered_stays_paused_with_its_full_timeout() {
        let mut h = Harness::new();
        h.notify(notification(1));
        h.advance(secs(4));
        h.send(Event::HoverStarted(1));

        let replaced = h.notify(notification(1).summary("again"));
        let while_hovered = h.advance(secs(60));
        h.send(Event::HoverEnded(1));

        assert!(replaced.contains(&Effect::CountdownPaused {
            id: 1,
            remaining: secs(5),
        }));
        assert!(while_hovered.is_empty(), "{while_hovered:?}");
        assert!(h.advance(millis(4_999)).is_empty());
        assert_eq!(closed(&h.advance(millis(1))), expired(1));
    }

    #[test]
    fn hovering_a_popup_that_never_expires_changes_nothing() {
        let mut h = Harness::new();
        h.notify(notification(1).urgency(Urgency::Critical));

        assert!(h.send(Event::HoverStarted(1)).is_empty());
        assert!(h.send(Event::HoverEnded(1)).is_empty());
        assert!(h.advance(secs(3_600)).is_empty());
    }

    #[test]
    fn hover_events_for_unknown_or_closed_popups_do_nothing() {
        let mut h = Harness::new();
        h.notify(notification(1));
        h.send(Event::UserDismissed(1));

        assert!(h.send(Event::HoverStarted(1)).is_empty());
        assert!(h.send(Event::HoverEnded(1)).is_empty());
        assert!(h.send(Event::HoverStarted(42)).is_empty());
        assert!(h.send(Event::HoverEnded(42)).is_empty());
    }

    #[test]
    fn a_popup_that_reuses_a_closed_hovered_id_counts_down_normally() {
        let mut h = Harness::new();
        h.notify(notification(1));
        h.send(Event::HoverStarted(1));
        h.send(Event::UserDismissed(1));

        h.notify(notification(1));

        assert_eq!(closed(&h.advance(secs(5))), expired(1));
    }

    #[test]
    fn a_stale_timer_for_a_closed_notification_does_nothing() {
        let mut h = Harness::new();
        h.notify(notification(1));
        h.send(Event::UserDismissed(1));

        assert!(h.send(Event::TimerFired(Timer::Expire(1))).is_empty());
        assert!(h.send(Event::TimerFired(Timer::Expire(99))).is_empty());
    }
}
