//! Seam 1, timeouts: how long a Popup stays up, from the sender's request,
//! its Urgency and the user's config.

use std::time::Duration;

use lucent::config::Config;
use lucent::core::{Core, Effect, Event, Notification, Timeout, Urgency};

/// A core whose Stack holds at most `max_visible` Popups, set as a user would.
fn showing_at_most(max_visible: usize) -> Core {
    let (config, problems) = Config::parse(&format!("general.max_visible = {max_visible}"));
    assert_eq!(problems, []);
    Core::with_config(config)
}

fn sent(urgency: Urgency, timeout: Timeout) -> Notification {
    Notification {
        summary: "s".into(),
        urgency,
        timeout,
        ..Default::default()
    }
}

/// The timers the effects start, as their durations.
fn timers(effects: &[Effect]) -> Vec<Duration> {
    effects
        .iter()
        .filter_map(|e| match e {
            Effect::StartTimer { after, .. } => Some(*after),
            _ => None,
        })
        .collect()
}

fn notify(core: &mut Core, n: Notification) -> Vec<Duration> {
    timers(&core.handle(Event::Notify(n)))
}

#[test]
fn an_app_requested_timeout_is_honoured_by_default() {
    let mut core = Core::new();
    let after = Timeout::After(Duration::from_millis(1200));

    assert_eq!(
        notify(&mut core, sent(Urgency::Normal, after)),
        [Duration::from_millis(1200)]
    );
    assert_eq!(
        notify(&mut core, sent(Urgency::Normal, Timeout::Default)),
        [Duration::from_secs(5)]
    );
    assert_eq!(notify(&mut core, sent(Urgency::Low, Timeout::Never)), []);
}

#[test]
fn a_critical_notification_never_expires_whatever_the_app_asks() {
    let mut core = Core::new();

    for timeout in [
        Timeout::Default,
        Timeout::After(Duration::from_secs(2)),
        Timeout::Never,
    ] {
        assert_eq!(notify(&mut core, sent(Urgency::Critical, timeout)), []);
    }
}

/// Feeds a normal Notification and returns the id it was accepted under.
fn shown(core: &mut Core) -> u32 {
    match core.handle(Event::Notify(sent(Urgency::Normal, Timeout::Default)))[0] {
        Effect::Accepted(id) => id,
        ref other => panic!("Notify must be answered with an id first, got {other:?}"),
    }
}

#[test]
fn a_queued_critical_notification_starts_no_timer_once_shown() {
    let mut core = showing_at_most(1);
    let first = shown(&mut core);
    let queued = sent(Urgency::Critical, Timeout::After(Duration::from_secs(1)));
    assert_eq!(notify(&mut core, queued), []);

    assert_eq!(timers(&core.handle(Event::TimerFired(first))), []);
}

#[test]
fn a_critical_replacement_stops_the_old_timer() {
    let mut core = Core::new();
    let id = shown(&mut core);

    let effects = core.handle(Event::Notify(Notification {
        replaces_id: id,
        ..sent(Urgency::Critical, Timeout::Default)
    }));

    assert_eq!(timers(&effects), []);
    assert!(effects.contains(&Effect::CancelTimer(id)), "{effects:?}");
}

#[test]
fn with_app_timeouts_ignored_the_configured_timeout_is_used() {
    let (config, problems) = Config::parse("[general]\ntimeout = 3000\nhonor_app_timeouts = false");
    assert_eq!(problems, []);
    let mut core = Core::with_config(config);

    for timeout in [
        Timeout::Default,
        Timeout::After(Duration::from_millis(1200)),
        Timeout::Never,
    ] {
        assert_eq!(
            notify(&mut core, sent(Urgency::Normal, timeout)),
            [Duration::from_secs(3)]
        );
    }
    assert_eq!(
        notify(&mut core, sent(Urgency::Critical, Timeout::Default)),
        []
    );
}
