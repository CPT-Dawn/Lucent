//! Seam 3: the D-Bus adapter choosing a Notification's image from `app_icon`
//! and the image hints, in the order the freedesktop spec gives: `image-data`, then
//! `image-path`, then `app_icon`, then the legacy `icon_data`.

use std::collections::VecDeque;
use std::path::PathBuf;

use dbus::arg::{PropMap, RefArg, Variant};
use lucent::dbus::image_from_hints;
use lucent::image::{self, Image};

/// An `(iiibiiay)` image hint, shaped as libdbus delivers a struct inside
/// a variant.
fn pixels(fields: (i32, i32, i32, bool, i32, i32), data: Vec<u8>) -> Variant<Box<dyn RefArg>> {
    let (w, h, stride, alpha, bits, channels) = fields;
    let fields: VecDeque<Box<dyn RefArg>> = VecDeque::from([
        Box::new(w) as Box<dyn RefArg>,
        Box::new(h),
        Box::new(stride),
        Box::new(alpha),
        Box::new(bits),
        Box::new(channels),
        Box::new(data),
    ]);
    Variant(Box::new(fields))
}

/// An image hint of `width`x`height` opaque RGB pixels.
fn rgb(width: i32, height: i32) -> Variant<Box<dyn RefArg>> {
    let data = vec![0x80u8; (width * height * 3) as usize];
    pixels((width, height, width * 3, false, 8, 3), data)
}

fn hints(entries: Vec<(&str, Variant<Box<dyn RefArg>>)>) -> PropMap {
    entries
        .into_iter()
        .map(|(k, v)| (k.to_owned(), v))
        .collect()
}

fn text(s: &str) -> Variant<Box<dyn RefArg>> {
    Variant(Box::new(s.to_owned()))
}

/// The size of the pixels chosen, to tell the hints apart.
fn size(image: Option<Image>) -> (u32, u32) {
    match image {
        Some(Image::Pixels(p)) => (p.width(), p.height()),
        other => panic!("expected pixels, got {other:?}"),
    }
}

#[test]
fn image_data_wins_over_everything_else() {
    let hints = hints(vec![
        ("icon_data", rgb(1, 2)),
        ("image-path", text("/tmp/photo.png")),
        ("image-data", rgb(2, 1)),
    ]);

    assert_eq!(size(image_from_hints("firefox", &hints)), (2, 1));
}

#[test]
fn image_path_comes_before_app_icon() {
    let hints = hints(vec![("image-path", text("/tmp/photo.png"))]);

    assert_eq!(
        image_from_hints("firefox", &hints),
        Some(Image::File(PathBuf::from("/tmp/photo.png")))
    );
}

#[test]
fn app_icon_comes_before_the_legacy_icon_data() {
    let hints = hints(vec![("icon_data", rgb(1, 2))]);

    assert_eq!(
        image_from_hints("firefox", &hints),
        Some(Image::Icon("firefox".into()))
    );
}

#[test]
fn icon_data_is_the_last_resort() {
    let hints = hints(vec![("icon_data", rgb(1, 2))]);

    assert_eq!(size(image_from_hints("", &hints)), (1, 2));
}

#[test]
fn nothing_to_show_is_no_image() {
    assert_eq!(image_from_hints("", &PropMap::new()), None);
}

#[test]
fn the_deprecated_underscore_names_still_count() {
    let data = hints(vec![("image_data", rgb(3, 1))]);
    assert_eq!(size(image_from_hints("firefox", &data)), (3, 1));

    let path = hints(vec![("image_path", text("dialog-information"))]);
    assert_eq!(
        image_from_hints("firefox", &path),
        Some(Image::Icon("dialog-information".into()))
    );
}

#[test]
fn a_broken_hint_falls_through_to_the_next_one() {
    // Claims 4x4 pixels but carries only one byte.
    let broken = pixels((4, 4, 12, false, 8, 3), vec![0]);
    let hints = hints(vec![("image-data", broken), ("image-path", text(""))]);

    assert_eq!(
        image_from_hints("firefox", &hints),
        Some(Image::Icon("firefox".into()))
    );
}

#[test]
fn paths_and_file_uris_name_files_and_anything_else_names_an_icon() {
    let none = PropMap::new();
    assert_eq!(
        image_from_hints("/usr/share/pixmaps/app.svg", &none),
        Some(Image::File("/usr/share/pixmaps/app.svg".into()))
    );
    assert_eq!(
        image_from_hints("file:///home/me/My%20Pictures/a.png", &none),
        Some(Image::File("/home/me/My Pictures/a.png".into()))
    );
    assert_eq!(
        image_from_hints("mail-unread", &none),
        Some(Image::Icon("mail-unread".into()))
    );
}

#[test]
fn image_data_is_premultiplied_argb() {
    // One half-transparent red pixel, RGBA.
    let pixel = pixels((1, 1, 4, true, 8, 4), vec![255, 0, 0, 128]);
    let hints = hints(vec![("image-data", pixel)]);

    let Some(Image::Pixels(p)) = image_from_hints("", &hints) else {
        panic!("expected pixels");
    };
    assert_eq!(p.data(), 0x8080_0000u32.to_ne_bytes());
}

#[test]
fn large_images_are_scaled_down_once_on_arrival() {
    let hints = hints(vec![("image-data", rgb(1024, 512))]);

    assert_eq!(
        size(image_from_hints("", &hints)),
        (image::MAX_SIDE, image::MAX_SIDE / 2)
    );
}
