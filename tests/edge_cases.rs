//! Edge cases from the pre-release audit: inputs a sender or a config can
//! produce that once broke a Popup or the daemon.
use std::sync::Arc;
use std::time::Duration;

use lucent::core::{Core, Effect, Event, Notification, Popup, Timeout, Urgency};
use lucent::draw;
use lucent::icon::Icons;
use lucent::image::{Image, Pixels, RawPixels};
use lucent::markup;
use lucent::theme::Theme;

fn popup(body: &str, body_lines: u32) -> Popup {
    Popup {
        id: 1,
        app_name: "app".into(),
        summary: "summary".into(),
        body: body.into(),
        image: None,
        actions: vec![],
        more: 0,
        body_lines,
        timeout: None,
        progress: None,
        theme: Theme::default(),
    }
}

#[test]
fn body_lines_caps_a_multi_paragraph_body() {
    let four = (1..=4)
        .map(|i| i.to_string())
        .collect::<Vec<_>>()
        .join("\n");
    let forty = (1..=400)
        .map(|i| i.to_string())
        .collect::<Vec<_>>()
        .join("\n");
    let h4 = draw::measure(&popup(&four, 4), None, 360);
    let h400 = draw::measure(&popup(&forty, 4), None, 360);
    assert_eq!(
        h4, h400,
        "body_lines=4 but a 400-line body measures {h400}px vs {h4}px"
    );
}

#[test]
fn pixels_with_64_bit_dimensions_are_rejected_not_a_panic() {
    let got = std::panic::catch_unwind(|| {
        Pixels::from_raw(RawPixels {
            width: 1 << 62,
            height: 1,
            rowstride: 0,
            has_alpha: true,
            bits_per_sample: 8,
            channels: 4,
            data: &[],
        })
    });
    assert!(
        matches!(got, Ok(None)),
        "from_raw panicked instead of returning None"
    );
}

#[test]
fn a_huge_icon_size_does_not_panic() {
    let px = Pixels::from_raw(RawPixels {
        width: 1,
        height: 1,
        rowstride: 4,
        has_alpha: true,
        bits_per_sample: 8,
        channels: 4,
        data: &[255, 0, 0, 255],
    })
    .unwrap();
    let image = Image::Pixels(Arc::new(px));
    let got =
        std::panic::catch_unwind(|| Icons::new(3_000_000_000, "hicolor").load(&image).is_some());
    assert!(got.is_ok(), "Icons::load panicked for icon_size = 3e9");
}

#[test]
fn a_hovered_critical_popup_replaced_by_a_normal_one_stays_paused() {
    let mut core = Core::new();
    let critical = Notification {
        summary: "a".into(),
        urgency: Urgency::Critical,
        ..Default::default()
    };
    let id = match core.handle(Event::Notify(critical))[0] {
        Effect::Accepted(id) => id,
        _ => unreachable!(),
    };
    core.handle(Event::HoverStart(id)); // pointer is on the popup
    let normal = Notification {
        summary: "b".into(),
        replaces_id: id,
        timeout: Timeout::After(Duration::from_millis(100)),
        ..Default::default()
    };
    let effects = core.handle(Event::Notify(normal));
    assert!(
        !effects
            .iter()
            .any(|e| matches!(e, Effect::StartTimer { .. })),
        "a timer started while the pointer is still on the popup: {effects:?}"
    );
}

#[test]
fn cleaned_markup_with_a_noncharacter_entity_still_parses() {
    for body in ["<b>x</b> &#xFFFE;", "<b>x</b> &#1;"] {
        let clean = markup::clean(body);
        assert!(
            pango::parse_markup(&clean, '\0').is_ok(),
            "{body:?} -> {clean:?} does not parse; draw.rs falls back to raw tags"
        );
    }
}

#[test]
fn a_self_closing_tag_does_not_bold_the_rest_of_the_body() {
    assert_eq!(markup::clean("a<b/>c"), "ac");
}

#[test]
fn a_huge_body_is_cut_before_it_is_laid_out() {
    let mut core = Core::new();
    let body = "é".repeat(1 << 20);
    let effects = core.handle(Event::Notify(Notification {
        summary: "s".repeat(1 << 16),
        body,
        ..Default::default()
    }));
    let Some(Effect::Stack(stack)) = effects.iter().find(|e| matches!(e, Effect::Stack(_))) else {
        panic!("no Stack in {effects:?}");
    };
    assert!(stack[0].body.len() <= lucent::core::MAX_BODY);
    assert!(stack[0].summary.len() <= lucent::core::MAX_SUMMARY);
}
