//! Seam 5: painting one Popup into pixels, with and without an icon, and
//! its Action button geometry. Pure Cairo and Pango; no compositor.

use cairo::{Context, Format, ImageSurface};
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
        image: None,
        actions: labels
            .iter()
            .map(|label| Action {
                key: label.to_lowercase(),
                label: (*label).into(),
            })
            .collect(),
        more: 0,
        body_lines: 4,
        timeout: None,
        progress: None,
        theme: Theme::default(),
    }
}

/// An opaque red square icon of `side` pixels.
fn red_icon(side: i32) -> ImageSurface {
    let icon = ImageSurface::create(Format::ARgb32, side, side).unwrap();
    let cr = Context::new(&icon).unwrap();
    cr.set_source_rgb(1.0, 0.0, 0.0);
    cr.paint().unwrap();
    drop(cr);
    icon
}

#[test]
fn a_popup_grows_to_fit_an_icon_taller_than_its_text() {
    let icon = red_icon(96);

    let plain = draw::measure(&popup(&[]), None, 360);
    let with_icon = draw::measure(&popup(&[]), Some(&icon), 360);

    assert!(plain < 96, "one short line should be shorter than the icon");
    assert!(with_icon >= 96 + 2 * 16, "{with_icon} leaves no room");
}

#[test]
fn the_icon_is_painted_at_the_top_left_inside_the_padding() {
    let icon = red_icon(48);
    let height = draw::measure(&popup(&[]), Some(&icon), 360);
    let mut target = ImageSurface::create(Format::ARgb32, 360, height as i32).unwrap();
    let cr = Context::new(&target).unwrap();

    draw::paint(&cr, &popup(&[]), Some(&icon), 360, height, None).unwrap();
    drop(cr);

    let stride = target.stride() as usize;
    let data = target.data().unwrap();
    let pixel = |x: usize, y: usize| {
        u32::from_ne_bytes(data[y * stride + x * 4..][..4].try_into().unwrap())
    };
    assert_eq!(pixel(16 + 24, 16 + 24), 0xFFFF_0000, "icon centre");
    assert_ne!(
        pixel(16 + 48 + 2, 16 + 24),
        0xFFFF_0000,
        "right of the icon"
    );
}

#[test]
fn a_popup_without_actions_has_no_buttons() {
    assert_eq!(draw::action_rects(&popup(&[]), None, WIDTH), []);
}

#[test]
fn up_to_three_buttons_share_one_row_below_the_text() {
    let without = draw::measure(&popup(&[]), None, WIDTH);
    let p = popup(&["Reply", "Archive", "Mark as read"]);

    let buttons = draw::action_rects(&p, None, WIDTH);

    assert_eq!(buttons.len(), 3);
    assert!(buttons.iter().all(|b| b.y == buttons[0].y), "{buttons:?}");
    assert!(buttons[0].y >= f64::from(without) - 16.0, "{buttons:?}");
    assert!(buttons.windows(2).all(|w| w[0].x + w[0].width <= w[1].x));
    assert!(draw::measure(&p, None, WIDTH) > without, "the Popup grows");
}

#[test]
fn a_fourth_button_wraps_to_a_new_row_and_all_stay_inside_the_popup() {
    let p = popup(&[
        "Reply",
        "Archive",
        "A label far too long to ever fit in a third of a popup",
        "Delete",
    ]);
    let height = f64::from(draw::measure(&p, None, WIDTH));

    let buttons = draw::action_rects(&p, None, WIDTH);

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
    let buttons = draw::action_rects(&p, None, WIDTH);
    let centre = |b: &draw::Rect| (b.x + b.width / 2.0, b.y + b.height / 2.0);

    let at = |x: f64, y: f64| buttons.iter().position(|b| b.contains(x, y));

    let (x, y) = centre(&buttons[1]);
    assert_eq!(at(x, y), Some(1));
    assert_eq!(at(1.0, 1.0), None, "the Popup's corner is no button");
}

#[test]
fn buttons_sit_below_an_icon_taller_than_the_text() {
    let icon = red_icon(96);
    let p = popup(&["Reply"]);

    let buttons = draw::action_rects(&p, Some(&icon), WIDTH);

    assert!(buttons[0].y >= 16.0 + 96.0, "{buttons:?}");
    assert!(f64::from(draw::measure(&p, Some(&icon), WIDTH)) >= buttons[0].y + buttons[0].height);
}
