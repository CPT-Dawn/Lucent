//! Seam 1: the Notification core. Events in, effects out; no Wayland or D-Bus.

use std::time::Duration;

use lucent::config::Config;
use lucent::core::{CloseReason, Core, Effect, Event, Notification, Popup};
use lucent::theme::Theme;

fn hello() -> Notification {
    Notification {
        app_name: "notify-send".into(),
        summary: "Hello".into(),
        body: "World".into(),
        ..Default::default()
    }
}

#[test]
fn notify_shows_a_popup_that_expires_after_the_default_timeout() {
    let mut core = Core::new();

    let effects = core.handle(Event::Notify(hello()));
    let id = match effects.first() {
        Some(Effect::Accepted(id)) => *id,
        other => panic!("Notify must be answered with an id first, got {other:?}"),
    };
    assert_eq!(
        effects[1..],
        [
            Effect::Stack(vec![Popup {
                id,
                app_name: "notify-send".into(),
                summary: "Hello".into(),
                body: "World".into(),
                more: 0,
                body_lines: 4,
                expire: Some(Duration::from_secs(5)),
                theme: Theme::default(),
            }]),
            Effect::StartTimer {
                id,
                after: Duration::from_secs(5),
            },
        ]
    );

    assert_eq!(
        core.handle(Event::TimerFired(id)),
        [
            Effect::Stack(vec![]),
            Effect::Closed {
                id,
                reason: CloseReason::Expired,
            },
        ]
    );
}

#[test]
fn the_configured_timeout_decides_when_a_popup_expires() {
    let (config, _) = Config::parse("general.timeout = 1500");
    let mut core = Core::with_config(config);

    let effects = core.handle(Event::Notify(hello()));

    assert!(
        effects.iter().any(|e| matches!(
            e,
            Effect::StartTimer { after, .. } if *after == Duration::from_millis(1500)
        )),
        "{effects:?}"
    );
}

fn titled(summary: &str) -> Notification {
    Notification {
        summary: summary.into(),
        ..hello()
    }
}

/// Feeds a Notify and returns the id it was accepted under.
fn notify(core: &mut Core, n: Notification) -> u32 {
    match core.handle(Event::Notify(n)).first() {
        Some(Effect::Accepted(id)) => *id,
        other => panic!("Notify must be answered with an id first, got {other:?}"),
    }
}

/// The last Stack in `effects`, as each Popup's summary and "+N more" count.
fn last_stack(effects: &[Effect]) -> Vec<(String, usize)> {
    effects
        .iter()
        .rev()
        .find_map(|e| match e {
            Effect::Stack(popups) => {
                Some(popups.iter().map(|p| (p.summary.clone(), p.more)).collect())
            }
            _ => None,
        })
        .expect("effects should include a Stack")
}

fn stack(entries: &[(&str, usize)]) -> Vec<(String, usize)> {
    entries.iter().map(|(s, n)| (s.to_string(), *n)).collect()
}

#[test]
fn the_newest_popup_sits_closest_to_the_anchor() {
    let mut core = Core::new();
    notify(&mut core, titled("first"));
    let effects = core.handle(Event::Notify(titled("second")));

    assert_eq!(last_stack(&effects), stack(&[("second", 0), ("first", 0)]));
}

fn starts_a_timer(effects: &[Effect]) -> bool {
    effects
        .iter()
        .any(|e| matches!(e, Effect::StartTimer { .. }))
}

#[test]
fn notifications_beyond_the_cap_wait_in_the_queue_behind_a_more_hint() {
    let mut core = Core::with_max_visible(2);
    notify(&mut core, titled("a"));
    notify(&mut core, titled("b"));
    notify(&mut core, titled("c"));
    let effects = core.handle(Event::Notify(titled("d")));

    assert_eq!(last_stack(&effects), stack(&[("b", 0), ("a", 2)]));
    assert!(
        !starts_a_timer(&effects),
        "a queued Notification's timeout starts only once it is shown: {effects:?}"
    );
}

#[test]
fn the_queue_drains_oldest_first_as_popups_expire() {
    let mut core = Core::with_max_visible(2);
    let a = notify(&mut core, titled("a"));
    let b = notify(&mut core, titled("b"));
    let c = notify(&mut core, titled("c"));
    notify(&mut core, titled("d"));

    let effects = core.handle(Event::TimerFired(a));
    assert_eq!(last_stack(&effects), stack(&[("c", 0), ("b", 1)]));
    assert!(effects.contains(&Effect::StartTimer {
        id: c,
        after: Duration::from_secs(5),
    }));

    let effects = core.handle(Event::TimerFired(b));
    assert_eq!(last_stack(&effects), stack(&[("d", 0), ("c", 0)]));
}

fn replacing(id: u32, summary: &str) -> Notification {
    Notification {
        replaces_id: id,
        ..titled(summary)
    }
}

#[test]
fn a_replacement_updates_its_popup_in_place_and_restarts_its_timeout() {
    let mut core = Core::new();
    let volume = notify(&mut core, titled("volume 40%"));
    notify(&mut core, titled("other"));

    let effects = core.handle(Event::Notify(replacing(volume, "volume 45%")));

    assert_eq!(effects.first(), Some(&Effect::Accepted(volume)));
    assert_eq!(
        last_stack(&effects),
        stack(&[("other", 0), ("volume 45%", 0)])
    );
    assert!(effects.contains(&Effect::StartTimer {
        id: volume,
        after: Duration::from_secs(5),
    }));
}

#[test]
fn a_replaces_id_that_matches_nothing_gets_a_fresh_id() {
    let mut core = Core::new();
    let gone = notify(&mut core, titled("gone"));
    core.handle(Event::TimerFired(gone));
    let live = notify(&mut core, titled("live"));

    let fresh = notify(&mut core, replacing(gone, "back"));
    let effects = core.handle(Event::Notify(replacing(4242, "stranger")));

    assert!(![gone, live].contains(&fresh), "{fresh} must be a new id");
    assert_eq!(
        last_stack(&effects),
        stack(&[("stranger", 0), ("back", 0), ("live", 0)])
    );
}

#[test]
fn a_replacement_for_a_queued_notification_updates_it_where_it_waits() {
    let mut core = Core::with_max_visible(1);
    let shown = notify(&mut core, titled("shown"));
    let waiting = notify(&mut core, titled("download 10%"));

    let effects = core.handle(Event::Notify(replacing(waiting, "download 90%")));
    assert_eq!(effects.first(), Some(&Effect::Accepted(waiting)));
    assert!(!starts_a_timer(&effects));

    let effects = core.handle(Event::TimerFired(shown));
    assert_eq!(last_stack(&effects), stack(&[("download 90%", 0)]));
}

fn closed(id: u32, reason: CloseReason) -> Effect {
    Effect::Closed { id, reason }
}

#[test]
fn closing_a_popup_cancels_its_timeout_and_makes_room_for_the_queue() {
    let mut core = Core::with_max_visible(1);
    let shown = notify(&mut core, titled("shown"));
    let waiting = notify(&mut core, titled("waiting"));

    let effects = core.handle(Event::Close(shown));

    assert_eq!(last_stack(&effects), stack(&[("waiting", 0)]));
    assert!(effects.contains(&Effect::CancelTimer(shown)));
    assert!(effects.contains(&Effect::StartTimer {
        id: waiting,
        after: Duration::from_secs(5),
    }));
    let closes: Vec<_> = effects
        .iter()
        .filter(|e| matches!(e, Effect::Closed { .. }))
        .collect();
    assert_eq!(closes, [&closed(shown, CloseReason::Closed)]);
}

#[test]
fn closing_a_queued_notification_shrinks_the_more_hint() {
    let mut core = Core::with_max_visible(1);
    notify(&mut core, titled("shown"));
    let waiting = notify(&mut core, titled("waiting"));
    notify(&mut core, titled("also waiting"));

    let effects = core.handle(Event::Close(waiting));

    assert_eq!(last_stack(&effects), stack(&[("shown", 1)]));
    assert!(effects.contains(&closed(waiting, CloseReason::Closed)));
}

#[test]
fn closing_an_unknown_or_already_closed_id_does_nothing() {
    let mut core = Core::new();
    let id = notify(&mut core, titled("once"));
    core.handle(Event::Close(id));

    assert_eq!(core.handle(Event::Close(id)), []);
    assert_eq!(core.handle(Event::Close(4242)), []);
    assert_eq!(core.handle(Event::TimerFired(id)), []);
}

#[test]
fn the_configured_max_visible_caps_the_stack() {
    let (config, _) = Config::parse("general.max_visible = 1");
    let mut core = Core::with_config(config);
    notify(&mut core, titled("a"));
    let effects = core.handle(Event::Notify(titled("b")));

    assert_eq!(last_stack(&effects), stack(&[("a", 1)]));
}
