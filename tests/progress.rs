//! Seam 1, progress half: the `value` hint in, the Popup's progress out.

use lucent::config::Config;
use lucent::core::{Core, Effect, Event, Notification};
use lucent::theme::{Rgba, Theme};

fn with_value(value: Option<i64>) -> Notification {
    Notification {
        app_name: "volume".into(),
        summary: "Volume".into(),
        value,
        ..Default::default()
    }
}

/// The progress the first Popup of the last Stack shows.
fn shown(effects: &[Effect]) -> Option<u8> {
    effects
        .iter()
        .rev()
        .find_map(|e| match e {
            Effect::Stack(popups) => Some(popups[0].value),
            _ => None,
        })
        .expect("a Notify should show a Popup")
}

#[test]
fn the_value_hint_appears_in_the_popup_clamped_to_0_through_100() {
    for (sent, expected) in [
        (Some(42), Some(42)),
        (Some(0), Some(0)),
        (Some(100), Some(100)),
        (Some(150), Some(100)),
        (Some(-5), Some(0)),
        (None, None),
    ] {
        let mut core = Core::new();
        let effects = core.handle(Event::Notify(with_value(sent)));
        assert_eq!(shown(&effects), expected, "value {sent:?}");
    }
}

#[test]
fn a_replacement_updates_the_progress_in_place() {
    let mut core = Core::new();
    let id = match core.handle(Event::Notify(with_value(Some(10))))[0] {
        Effect::Accepted(id) => id,
        ref other => panic!("expected an id, got {other:?}"),
    };

    let effects = core.handle(Event::Notify(Notification {
        replaces_id: id,
        ..with_value(Some(70))
    }));

    assert_eq!(effects[0], Effect::Accepted(id));
    assert_eq!(shown(&effects), Some(70));
}

#[test]
fn midnight_bloom_fills_progress_in_mint_over_a_faint_track() {
    let theme = Theme::midnight_bloom();
    assert_eq!(theme.progress_fill, Rgba::new(0x8D, 0xE3, 0xDC, 255));
    assert_eq!(theme.progress_track, Rgba::new(252, 253, 253, 31));
}

#[test]
fn track_and_fill_colours_are_theme_fields() {
    let (config, problems) =
        Config::parse("[theme]\nprogress_track = \"#112233\"\nprogress_fill = \"#445566\"");
    assert_eq!(problems, []);
    let theme = config.theme();
    assert_eq!(theme.progress_track, Rgba::new(0x11, 0x22, 0x33, 255));
    assert_eq!(theme.progress_fill, Rgba::new(0x44, 0x55, 0x66, 255));
}
