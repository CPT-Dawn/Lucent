//! Seam 4: loading a Notification's image for drawing. Files, theme icons and
//! inline pixels all come back as one Cairo surface at the Theme's icon size,
//! or not at all.

#[path = "support/icons.rs"]
mod support;

use lucent::icon::Icons;
use lucent::image::{Image, from_hints};
use support::{RED, RED_SVG, SIZE, inspect, write_png};

#[test]
fn a_png_file_is_scaled_to_the_icon_size() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("wide.png");
    write_png(&path);

    let icon = Icons::new(SIZE, "hicolor").load(&Image::File(path));

    assert_eq!(inspect(icon.unwrap()), ((48, 24), RED));
}

#[test]
fn an_svg_file_is_rendered_at_the_icon_size() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("red.svg");
    std::fs::write(&path, RED_SVG).unwrap();

    let icon = Icons::new(SIZE, "hicolor").load(&Image::File(path));

    assert_eq!(inspect(icon.unwrap()), ((48, 48), RED));
}

#[test]
fn inline_pixels_are_scaled_to_the_icon_size() {
    let pixels = dbus::arg::Variant(Box::new(std::collections::VecDeque::from([
        Box::new(2i32) as Box<dyn dbus::arg::RefArg>,
        Box::new(2i32),
        Box::new(6i32),
        Box::new(false),
        Box::new(8i32),
        Box::new(3i32),
        Box::new([255u8, 0, 0].repeat(4)),
    ])) as Box<dyn dbus::arg::RefArg>);
    let hints = [("image-data".to_owned(), pixels)].into_iter().collect();
    let image = from_hints("", &hints).unwrap();

    let icon = Icons::new(SIZE, "hicolor").load(&image);

    assert_eq!(inspect(icon.unwrap()), ((48, 48), RED));
}

#[test]
fn missing_or_broken_images_load_as_nothing() {
    let dir = tempfile::tempdir().unwrap();
    let broken_png = dir.path().join("broken.png");
    let broken_svg = dir.path().join("broken.svg");
    std::fs::write(&broken_png, "not a png").unwrap();
    std::fs::write(&broken_svg, "<svg").unwrap();
    let icons = Icons::new(SIZE, "hicolor");

    for image in [
        Image::File(dir.path().join("missing.png")),
        Image::File(broken_png),
        Image::File(broken_svg),
        Image::File(dir.path().to_owned()),
        Image::Icon("lucent-no-such-icon-anywhere".into()),
    ] {
        assert!(icons.load(&image).is_none(), "{image:?} should not load");
    }
}
