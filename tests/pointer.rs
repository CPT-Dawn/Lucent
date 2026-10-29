//! Seam 1, pointer half: clicks and hover as core events. No Wayland: the
//! renderer only translates pointer input into these events.

use std::cell::Cell;
use std::rc::Rc;
use std::time::{Duration, Instant};

use lucent::config::Config;
use lucent::core::{Action, Button, CloseReason, Core, Effect, Event, Notification, Urgency};

fn with_actions(summary: &str, actions: &[(&str, &str)]) -> Notification {
    Notification {
        app_name: "mail".into(),
        summary: summary.into(),
        body: String::new(),
        replaces_id: 0,
        urgency: Urgency::Normal,
        actions: actions
            .iter()
            .map(|(key, label)| Action {
                key: (*key).into(),
                label: (*label).into(),
            })
            .collect(),
    }
}

fn notify(core: &mut Core, n: Notification) -> u32 {
    match core.handle(Event::Notify(n)).first() {
        Some(Effect::Accepted(id)) => *id,
        other => panic!("expected an id, got {other:?}"),
    }
}

fn dismissed(id: u32) -> Effect {
    Effect::Closed {
        id,
        reason: CloseReason::Dismissed,
    }
}

/// The signals a batch of effects asks the D-Bus adapter to emit, in order.
fn signals(effects: &[Effect]) -> Vec<Effect> {
    effects
        .iter()
        .filter(|e| matches!(e, Effect::ActionInvoked { .. } | Effect::Closed { .. }))
        .cloned()
        .collect()
}

/// The ids in the last Stack a batch of effects shows.
fn stack_ids(effects: &[Effect]) -> Vec<u32> {
    effects
        .iter()
        .rev()
        .find_map(|e| match e {
            Effect::Stack(stack) => Some(stack.iter().map(|p| p.id).collect()),
            _ => None,
        })
        .expect("the Stack should change")
}

#[test]
fn left_click_runs_the_default_action_then_dismisses() {
    let mut core = Core::new();
    let other = notify(&mut core, with_actions("other", &[]));
    let id = notify(&mut core, with_actions("mail", &[("default", "Open")]));

    let effects = core.handle(Event::Click {
        id,
        button: Button::Left,
    });

    assert_eq!(
        signals(&effects),
        [
            Effect::ActionInvoked {
                id,
                key: "default".into(),
            },
            dismissed(id),
        ]
    );
    assert_eq!(stack_ids(&effects), [other]);
}

#[test]
fn middle_click_dismisses_without_running_anything() {
    let mut core = Core::new();
    let id = notify(&mut core, with_actions("mail", &[("default", "Open")]));

    let effects = core.handle(Event::Click {
        id,
        button: Button::Middle,
    });

    assert_eq!(signals(&effects), [dismissed(id)]);
    assert_eq!(stack_ids(&effects), []);
}

#[test]
fn right_click_dismisses_every_popup_and_the_queue_without_running_anything() {
    let mut core = Core::with_max_visible(2);
    let first = notify(&mut core, with_actions("first", &[("default", "Open")]));
    let second = notify(&mut core, with_actions("second", &[]));
    let queued = notify(&mut core, with_actions("queued", &[]));

    let effects = core.handle(Event::Click {
        id: first,
        button: Button::Right,
    });

    assert_eq!(
        signals(&effects),
        [dismissed(second), dismissed(first), dismissed(queued)]
    );
    assert_eq!(stack_ids(&effects), []);
    assert!(
        !effects
            .iter()
            .any(|e| matches!(e, Effect::StartTimer { .. })),
        "nothing should be promoted: {effects:?}"
    );
    assert_eq!(core.handle(Event::TimerFired(first)), []);
}

#[test]
fn rebinding_a_button_in_general_changes_what_it_does() {
    let (config, problems) = Config::parse(
        r#"
        [general]
        left_click = "dismiss"
        middle_click = "invoke-default-action"
        right_click = "none"
        "#,
    );
    assert_eq!(problems, []);
    let mut core = Core::with_config(config);
    let left = notify(&mut core, with_actions("left", &[("default", "Open")]));
    let middle = notify(&mut core, with_actions("middle", &[("default", "Open")]));

    let right_click = core.handle(Event::Click {
        id: left,
        button: Button::Right,
    });
    let left_click = core.handle(Event::Click {
        id: left,
        button: Button::Left,
    });
    let middle_click = core.handle(Event::Click {
        id: middle,
        button: Button::Middle,
    });

    assert_eq!(right_click, [], "`none` leaves the Popup alone");
    assert_eq!(signals(&left_click), [dismissed(left)]);
    assert_eq!(
        signals(&middle_click),
        [
            Effect::ActionInvoked {
                id: middle,
                key: "default".into(),
            },
            dismissed(middle),
        ]
    );
}

/// A clock the test moves by hand.
#[derive(Clone)]
struct FakeClock(Rc<Cell<Instant>>);

impl FakeClock {
    fn new() -> Self {
        Self(Rc::new(Cell::new(Instant::now())))
    }

    fn advance(&self, by: Duration) {
        self.0.set(self.0.get() + by);
    }

    fn core(&self) -> Core {
        let now = self.0.clone();
        Core::new().with_clock(move || now.get())
    }
}

#[test]
fn hovering_pauses_the_timeout_and_leaving_resumes_the_remaining_time() {
    let clock = FakeClock::new();
    let mut core = clock.core();
    let id = notify(&mut core, with_actions("mail", &[]));

    clock.advance(Duration::from_secs(2));
    assert_eq!(
        core.handle(Event::HoverStart(id)),
        [Effect::CancelTimer(id)]
    );

    clock.advance(Duration::from_secs(60));
    assert_eq!(
        core.handle(Event::HoverEnd(id)),
        [Effect::StartTimer {
            id,
            after: Duration::from_secs(3),
        }],
        "the default 5 s minus the 2 s already shown, not a full reset"
    );

    clock.advance(Duration::from_secs(1));
    core.handle(Event::HoverStart(id));
    clock.advance(Duration::from_secs(1));
    assert_eq!(
        core.handle(Event::HoverEnd(id)),
        [Effect::StartTimer {
            id,
            after: Duration::from_secs(2),
        }],
        "a second hover pauses what the first one left"
    );
}

#[test]
fn a_hovered_popup_does_not_expire() {
    let clock = FakeClock::new();
    let mut core = clock.core();
    let id = notify(&mut core, with_actions("mail", &[]));

    core.handle(Event::HoverStart(id));

    assert_eq!(core.handle(Event::TimerFired(id)), []);
}
