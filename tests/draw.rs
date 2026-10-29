//! Seam 5: painting one Popup into pixels, with and without an icon.

use cairo::{Context, Format, ImageSurface};
use lucent::core::Popup;
use lucent::draw;
use lucent::theme::Theme;

fn popup() -> Popup {
    Popup {
        id: 1,
        app_name: String::new(),
        summary: "Hi".into(),
        body: String::new(),
        image: None,
        more: 0,
        body_lines: 4,
        expire: None,
        value: None,
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

    let plain = draw::measure(&popup(), None, 360);
    let with_icon = draw::measure(&popup(), Some(&icon), 360);

    assert!(plain < 96, "one short line should be shorter than the icon");
    assert!(with_icon >= 96 + 2 * 16, "{with_icon} leaves no room");
}

#[test]
fn the_icon_is_painted_at_the_top_left_inside_the_padding() {
    let icon = red_icon(48);
    let height = draw::measure(&popup(), Some(&icon), 360);
    let mut target = ImageSurface::create(Format::ARgb32, 360, height as i32).unwrap();
    let cr = Context::new(&target).unwrap();

    draw::paint(&cr, &popup(), Some(&icon), 360, height).unwrap();
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
