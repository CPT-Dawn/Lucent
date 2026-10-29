//! Seam 1, Action half: Actions as core events. No Wayland: the renderer
//! only turns a press on an Action into [`Event::InvokeAction`].

use lucent::core::{Action, CloseReason, Core, Effect, Event, Notification};

fn with_actions(summary: &str, actions: &[(&str, &str)]) -> Notification {
    Notification {
        app_name: "mail".into(),
        summary: summary.into(),
        actions: actions
            .iter()
            .map(|(key, label)| Action {
                key: (*key).into(),
                label: (*label).into(),
            })
            .collect(),
        ..Default::default()
    }
}

fn notify(core: &mut Core, n: Notification) -> u32 {
    match core.handle(Event::Notify(n)).first() {
        Some(Effect::Accepted(id)) => *id,
        other => panic!("expected an id, got {other:?}"),
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

fn invoke(id: u32, key: &str) -> Event {
    Event::InvokeAction {
        id,
        key: key.into(),
    }
}

#[test]
fn clicking_an_action_invokes_its_key_then_dismisses() {
    let mut core = Core::new();
    let id = notify(
        &mut core,
        with_actions("mail", &[("default", "Open"), ("reply", "Reply")]),
    );

    let effects = core.handle(invoke(id, "reply"));

    assert_eq!(
        signals(&effects),
        [
            Effect::ActionInvoked {
                id,
                key: "reply".into(),
            },
            Effect::Closed {
                id,
                reason: CloseReason::Dismissed,
            },
        ]
    );
    assert!(effects.contains(&Effect::CancelTimer(id)), "{effects:?}");
    assert!(effects.contains(&Effect::Stack(vec![])), "{effects:?}");
}

/// The Actions the last Stack in a batch of effects shows on `id`'s Popup.
fn shown_actions(effects: &[Effect], id: u32) -> Vec<String> {
    effects
        .iter()
        .rev()
        .find_map(|e| match e {
            Effect::Stack(stack) => stack.iter().find(|p| p.id == id).cloned(),
            _ => None,
        })
        .expect("the Popup should be shown")
        .actions
        .into_iter()
        .map(|a| a.label)
        .collect()
}

#[test]
fn the_popup_shows_every_action_but_the_default_in_order() {
    let mut core = Core::new();

    let effects = core.handle(Event::Notify(with_actions(
        "mail",
        &[
            ("reply", "Reply"),
            ("default", "Open"),
            ("archive", "Archive"),
        ],
    )));
    let id = match effects.first() {
        Some(Effect::Accepted(id)) => *id,
        other => panic!("expected an id, got {other:?}"),
    };

    assert_eq!(shown_actions(&effects, id), ["Reply", "Archive"]);
}

#[test]
fn a_replacement_swaps_the_actions_for_its_own() {
    let mut core = Core::new();
    let id = notify(&mut core, with_actions("mail", &[("reply", "Reply")]));

    let mut replacement = with_actions("mail", &[("snooze", "Snooze")]);
    replacement.replaces_id = id;
    let effects = core.handle(Event::Notify(replacement));

    assert_eq!(shown_actions(&effects, id), ["Snooze"]);
    assert_eq!(core.handle(invoke(id, "reply")), [], "reply is gone");
}
