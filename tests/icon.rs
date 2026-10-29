//! Seam 4: loading a Notification's image for drawing. Files, theme icons and
//! inline pixels all come back as one Cairo surface at the Theme's icon size,
//! or not at all.

#[path = "support/icons.rs"]
mod support;

use lucent::dbus::image_from_hints;
use lucent::icon::Icons;
use lucent::image::Image;
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
    let image = image_from_hints("", &hints).unwrap();

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

#[test]
fn a_png_declaring_a_huge_size_is_not_decoded() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("bomb.png");
    // A valid signature and IHDR claiming 100000×100000, and nothing else.
    let mut png = b"\x89PNG\r\n\x1a\n\0\0\0\rIHDR".to_vec();
    png.extend(100_000u32.to_be_bytes());
    png.extend(100_000u32.to_be_bytes());
    png.extend([8, 6, 0, 0, 0, 0, 0, 0, 0]);
    std::fs::write(&path, png).unwrap();

    assert!(
        Icons::new(SIZE, "hicolor")
            .load(&Image::File(path))
            .is_none()
    );
}

#[test]
fn icons_on_a_scaled_monitor_hold_more_pixels_at_the_same_size() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("red.svg");
    std::fs::write(&path, RED_SVG).unwrap();

    let icon = Icons::new(SIZE, "hicolor")
        .with_scale(2.0)
        .load(&Image::File(path))
        .unwrap();

    assert_eq!((icon.width(), icon.height()), (96, 96));
    assert_eq!(lucent::icon::extent(&icon), (48.0, 48.0));
}
