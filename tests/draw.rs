//! Seam 3: Action button geometry. Pure Cairo and Pango; no compositor.

use lucent::core::{Action, Popup};
use lucent::draw;
use lucent::theme::Theme;

const WIDTH: u32 = 360;

fn popup(labels: &[&str]) -> Popup {
    Popup {
        id: 1,
        app_name: "mail".into(),
        summary: "New mail".into(),
        body: "From someone".into(),
        actions: labels
            .iter()
            .map(|label| Action {
                key: label.to_lowercase(),
                label: (*label).into(),
            })
            .collect(),
        more: 0,
        body_lines: 4,
        expire: None,
        theme: Theme::default(),
    }
}

#[test]
fn a_popup_without_actions_has_no_buttons() {
    assert_eq!(draw::buttons(&popup(&[]), WIDTH), []);
}

#[test]
fn up_to_three_buttons_share_one_row_below_the_text() {
    let without = draw::measure(&popup(&[]), WIDTH);
    let p = popup(&["Reply", "Archive", "Mark as read"]);

    let buttons = draw::buttons(&p, WIDTH);

    assert_eq!(buttons.len(), 3);
    assert!(buttons.iter().all(|b| b.y == buttons[0].y), "{buttons:?}");
    assert!(buttons[0].y >= f64::from(without) - 16.0, "{buttons:?}");
    assert!(buttons.windows(2).all(|w| w[0].x + w[0].width <= w[1].x));
    assert!(draw::measure(&p, WIDTH) > without, "the Popup grows");
}

#[test]
fn a_fourth_button_wraps_to_a_new_row_and_all_stay_inside_the_popup() {
    let p = popup(&[
        "Reply",
        "Archive",
        "A label far too long to ever fit in a third of a popup",
        "Delete",
    ]);
    let height = f64::from(draw::measure(&p, WIDTH));

    let buttons = draw::buttons(&p, WIDTH);

    assert_eq!(buttons.len(), 4);
    assert!(buttons[3].y > buttons[0].y, "{buttons:?}");
    assert_eq!(buttons[2].width, buttons[0].width, "long labels truncate");
    for b in &buttons {
        assert!(b.x >= 0.0 && b.x + b.width <= f64::from(WIDTH), "{b:?}");
        assert!(b.y + b.height <= height, "{b:?} within {height}");
    }
}

#[test]
fn hit_testing_finds_the_button_under_the_pointer() {
    let p = popup(&["Reply", "Archive"]);
    let buttons = draw::buttons(&p, WIDTH);
    let centre = |b: &draw::Rect| (b.x + b.width / 2.0, b.y + b.height / 2.0);

    let at = |x: f64, y: f64| buttons.iter().position(|b| b.contains(x, y));

    let (x, y) = centre(&buttons[1]);
    assert_eq!(at(x, y), Some(1));
    assert_eq!(at(1.0, 1.0), None, "the Popup's corner is no button");
}
