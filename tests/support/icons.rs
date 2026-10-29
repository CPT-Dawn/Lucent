//! Fixtures shared by the icon-loading suites.

use std::path::Path;

use cairo::{Context, Format, ImageSurface};

pub const SIZE: u32 = 48;

/// A 16x8 PNG filled with opaque red.
pub fn write_png(path: &Path) {
    let surface = ImageSurface::create(Format::ARgb32, 16, 8).unwrap();
    let cr = Context::new(&surface).unwrap();
    cr.set_source_rgb(1.0, 0.0, 0.0);
    cr.paint().unwrap();
    drop(cr);
    surface
        .write_to_png(&mut std::fs::File::create(path).unwrap())
        .unwrap();
}

pub const RED_SVG: &str = r##"<svg xmlns="http://www.w3.org/2000/svg" width="10" height="10">
<rect width="10" height="10" fill="#ff0000"/></svg>"##;

/// The size of a loaded icon and its centre pixel as premultiplied ARGB.
pub fn inspect(surface: ImageSurface) -> ((i32, i32), u32) {
    let (w, h) = (surface.width(), surface.height());
    let stride = surface.stride() as usize;
    let mut surface = surface;
    let data = surface.data().unwrap();
    let at = (h as usize / 2) * stride + (w as usize / 2) * 4;
    let pixel = u32::from_ne_bytes(data[at..at + 4].try_into().unwrap());
    ((w, h), pixel)
}

pub const RED: u32 = 0xFFFF_0000;
