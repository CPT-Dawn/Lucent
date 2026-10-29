//! Seam 4: loading a Notification's image for drawing. Files, theme icons and
//! inline pixels all come back as one Cairo surface at the Theme's icon size,
//! or not at all.

use std::path::Path;

use cairo::{Context, Format, ImageSurface};
use lucent::icon::Icons;
use lucent::image::{Image, from_hints};

const SIZE: u32 = 48;

/// A 16x8 PNG filled with opaque red.
fn write_png(path: &Path) {
    let surface = ImageSurface::create(Format::ARgb32, 16, 8).unwrap();
    let cr = Context::new(&surface).unwrap();
    cr.set_source_rgb(1.0, 0.0, 0.0);
    cr.paint().unwrap();
    drop(cr);
    surface
        .write_to_png(&mut std::fs::File::create(path).unwrap())
        .unwrap();
}

const RED_SVG: &str = r##"<svg xmlns="http://www.w3.org/2000/svg" width="10" height="10">
<rect width="10" height="10" fill="#ff0000"/></svg>"##;

/// The size of a loaded icon and its centre pixel as premultiplied ARGB.
fn inspect(surface: ImageSurface) -> ((i32, i32), u32) {
    let (w, h) = (surface.width(), surface.height());
    let stride = surface.stride() as usize;
    let mut surface = surface;
    let data = surface.data().unwrap();
    let at = (h as usize / 2) * stride + (w as usize / 2) * 4;
    let pixel = u32::from_ne_bytes(data[at..at + 4].try_into().unwrap());
    ((w, h), pixel)
}

const RED: u32 = 0xFFFF_0000;

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
fn an_icon_name_is_found_in_the_icon_theme() {
    let root = tempfile::tempdir().unwrap();
    let theme = root.path().join("Lucent-Test");
    std::fs::create_dir_all(theme.join("48x48/apps")).unwrap();
    std::fs::create_dir_all(theme.join("scalable/apps")).unwrap();
    std::fs::write(
        theme.join("index.theme"),
        "[Icon Theme]\nName=Lucent-Test\nDirectories=48x48/apps,scalable/apps\n\n\
         [48x48/apps]\nSize=48\nType=Fixed\n\n\
         [scalable/apps]\nSize=16\nMinSize=8\nMaxSize=24\nType=Scalable\n",
    )
    .unwrap();
    write_png(&theme.join("48x48/apps/lucent-png.png"));
    std::fs::write(theme.join("scalable/apps/lucent-svg.svg"), RED_SVG).unwrap();
    let icons = Icons::new(SIZE, "Lucent-Test").with_search_path(root.path());

    let png = icons.load(&Image::Icon("lucent-png".into()));
    // Only offered for smaller sizes, so found by the any-size fallback.
    let svg = icons.load(&Image::Icon("lucent-svg".into()));

    assert_eq!(inspect(png.unwrap()), ((48, 24), RED));
    assert_eq!(inspect(svg.unwrap()), ((48, 48), RED));
}

#[test]
fn missing_or_broken_images_load_as_nothing() {
    let dir = tempfile::tempdir().unwrap();
    let broken_png = dir.path().join("broken.png");
    let broken_svg = dir.path().join("broken.svg");
    std::fs::write(&broken_png, "not a png").unwrap();
    std::fs::write(&broken_svg, "<svg").unwrap();
    let icons = Icons::new(SIZE, "hicolor").with_search_path(dir.path());

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
