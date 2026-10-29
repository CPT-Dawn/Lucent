//! Seam 1: the Notification core. Events in, effects out; no Wayland or D-Bus.

use std::time::Duration;

use lucent::core::{CloseReason, Core, Effect, Event, Notification, Popup};

fn hello() -> Notification {
    Notification {
        app_name: "notify-send".into(),
        summary: "Hello".into(),
        body: "World".into(),
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
