//! Timeouts: how long each Popup stays, its expiry timer and (later) hover
//! pause.

use std::collections::HashMap;
use std::time::{Duration, Instant};

use super::{CloseReason, Effect, Engine, Expiry, Notification, NotificationId, Popup, Timer};

/// When each visible Popup is due to expire.
#[derive(Debug, Default)]
pub(super) struct Deadlines(HashMap<NotificationId, Instant>);

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

    /// (Re)start a Popup's countdown from its full timeout.
    pub(super) fn start_expiry(&mut self, popup: &Popup, out: &mut Vec<Effect>) {
        match popup.timeout {
            Some(after) => {
                self.expiry.0.insert(popup.id(), self.clock.now() + after);
                out.push(Effect::ScheduleTimer {
                    timer: Timer::Expire(popup.id()),
                    after,
                });
            }
            None => self.stop_expiry(popup.id(), out),
        }
    }

    /// Forget a Popup's countdown.
    pub(super) fn stop_expiry(&mut self, id: NotificationId, out: &mut Vec<Effect>) {
        if self.expiry.0.remove(&id).is_some() {
            out.push(Effect::CancelTimer(Timer::Expire(id)));
        }
    }

    pub(super) fn timer_fired(&mut self, timer: Timer, out: &mut Vec<Effect>) {
        match timer {
            Timer::Expire(id) => {
                let Some(&deadline) = self.expiry.0.get(&id) else {
                    return;
                };
                let now = self.clock.now();
                if now >= deadline {
                    self.expiry.0.remove(&id);
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

    /// Not handled yet: ticket #4 pauses the countdown while hovered.
    pub(super) fn hover_started(&mut self, _id: NotificationId, _out: &mut Vec<Effect>) {}

    /// Not handled yet: ticket #4 resumes the countdown.
    pub(super) fn hover_ended(&mut self, _id: NotificationId, _out: &mut Vec<Effect>) {}
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
    fn a_stale_timer_for_a_closed_notification_does_nothing() {
        let mut h = Harness::new();
        h.notify(notification(1));
        h.send(Event::UserDismissed(1));

        assert!(h.send(Event::TimerFired(Timer::Expire(1))).is_empty());
        assert!(h.send(Event::TimerFired(Timer::Expire(99))).is_empty());
    }
}
