//! Loads a Notification's [`Image`] into a Cairo surface at the Theme's icon
//! size: PNG through Cairo, SVG through `resvg`, names through the
//! freedesktop icon theme (`linicon`).
//!
//! An image is loaded once, when its Popup opens or its image changes, and the
//! surface lives only as long as that Popup. Anything that cannot be loaded
//! is logged at debug level and the Popup is shown without an icon.

use std::io::Cursor;
use std::path::{Path, PathBuf};

use cairo::{Context, Filter, Format, ImageSurface, SurfacePattern};
use resvg::{tiny_skia, usvg};

use crate::debug;
use crate::image::{Image, Pixels};

/// Image files larger than this are not read at all.
const MAX_FILE_BYTES: u64 = 16 << 20;

/// The fallback theme every icon theme inherits from.
const HICOLOR: &str = "hicolor";

/// Loads images at one icon size from one icon theme.
#[derive(Debug, Clone)]
pub struct Icons {
    size: u32,
    theme: String,
    /// Searched for icon themes before the standard XDG locations.
    search_paths: Vec<String>,
}

impl Icons {
    /// Loads at `size` pixels from `theme`; an empty `theme` means the GTK
    /// icon theme, else `hicolor`.
    pub fn new(size: u32, theme: &str) -> Self {
        let theme = if theme.is_empty() {
            gtk_icon_theme().unwrap_or_else(|| HICOLOR.to_owned())
        } else {
            theme.to_owned()
        };
        Self {
            size: size.max(1),
            theme,
            search_paths: Vec::new(),
        }
    }

    /// Also looks for icon themes under `path`, before anywhere else.
    pub fn with_search_path(mut self, path: impl AsRef<Path>) -> Self {
        self.search_paths
            .push(path.as_ref().to_string_lossy().into_owned());
        self
    }

    /// The image scaled so its longest side is the icon size, or `None` if
    /// it cannot be found or read.
    pub fn load(&self, image: &Image) -> Option<ImageSurface> {
        let loaded = match image {
            Image::Pixels(pixels) => self.load_pixels(pixels),
            Image::File(path) => self.load_file(path),
            Image::Icon(name) => self
                .lookup(name)
                .ok_or_else(|| format!("no icon named {name:?} in {:?}", self.theme))
                .and_then(|path| self.load_file(&path)),
        };
        loaded
            .inspect_err(|e| debug!("showing the popup without an icon: {e}"))
            .ok()
    }

    /// The icon file for `name`: the best match at the icon size, else the
    /// closest any size, else one in `/usr/share/pixmaps`.
    fn lookup(&self, name: &str) -> Option<PathBuf> {
        let search = || {
            let icons = linicon::lookup_icon(name)
                .from_theme(&self.theme)
                .with_scale(1);
            match icons.with_search_paths(&self.search_paths) {
                Ok(icons) => icons,
                // Only fails with path expansion, which is not enabled.
                Err(_) => linicon::lookup_icon(name).from_theme(&self.theme),
            }
        };
        let usable = |icon: &linicon::IconPath| icon.icon_type != linicon::IconType::XMP;
        let size = u16::try_from(self.size).unwrap_or(u16::MAX);
        if let Some(icon) = search().with_size(size).filter_map(Result::ok).find(usable) {
            return Some(icon.path);
        }
        // Prefer a vector, then the smallest bitmap at least as big as the
        // icon, then the biggest one there is.
        let rank = |icon: &linicon::IconPath| {
            let svg = icon.icon_type == linicon::IconType::SVG;
            let big_enough = icon.max_size >= size;
            let closeness = if big_enough {
                u16::MAX - icon.max_size
            } else {
                icon.max_size
            };
            (svg, big_enough, closeness)
        };
        let any_size = search()
            .filter_map(Result::ok)
            .filter(usable)
            .max_by_key(rank)
            .map(|icon| icon.path);
        any_size.or_else(|| {
            ["png", "svg"]
                .iter()
                .map(|ext| Path::new("/usr/share/pixmaps").join(format!("{name}.{ext}")))
                .find(|path| path.is_file())
        })
    }

    fn load_pixels(&self, pixels: &Pixels) -> Result<ImageSurface, String> {
        let (w, h) = (pixels.width() as i32, pixels.height() as i32);
        let mut surface = ImageSurface::create(Format::ARgb32, w, h).map_err(|e| e.to_string())?;
        let stride = surface.stride() as usize;
        {
            let mut data = surface.data().map_err(|e| e.to_string())?;
            let row = w as usize * 4;
            for (y, src) in pixels.data().chunks_exact(row).enumerate() {
                data[y * stride..y * stride + row].copy_from_slice(src);
            }
        }
        self.fit(&surface)
    }

    fn load_file(&self, path: &Path) -> Result<ImageSurface, String> {
        let fail = |e: &dyn std::fmt::Display| format!("{}: {e}", path.display());
        let meta = std::fs::metadata(path).map_err(|e| fail(&e))?;
        if !meta.is_file() || meta.len() > MAX_FILE_BYTES {
            return Err(fail(&"not a regular file of a sensible size"));
        }
        let bytes = std::fs::read(path).map_err(|e| fail(&e))?;
        if bytes.starts_with(b"\x89PNG") {
            let png =
                ImageSurface::create_from_png(&mut Cursor::new(bytes)).map_err(|e| fail(&e))?;
            self.fit(&png)
        } else if is_svg(path, &bytes) {
            self.load_svg(&bytes).map_err(|e| fail(&e))
        } else {
            Err(fail(&"only PNG and SVG images are supported"))
        }
    }

    fn load_svg(&self, bytes: &[u8]) -> Result<ImageSurface, String> {
        let tree =
            usvg::Tree::from_data(bytes, &usvg::Options::default()).map_err(|e| e.to_string())?;
        let svg = tree.size();
        let (w, h) = self.fitted(svg.width() as f64, svg.height() as f64);
        let mut pixmap = tiny_skia::Pixmap::new(w as u32, h as u32).ok_or("empty SVG")?;
        let transform =
            tiny_skia::Transform::from_scale(w as f32 / svg.width(), h as f32 / svg.height());
        resvg::render(&tree, transform, &mut pixmap.as_mut());

        // tiny-skia gives premultiplied RGBA bytes; Cairo wants premultiplied
        // native-endian ARGB words.
        let mut surface = ImageSurface::create(Format::ARgb32, w, h).map_err(|e| e.to_string())?;
        let stride = surface.stride() as usize;
        {
            let mut data = surface.data().map_err(|e| e.to_string())?;
            for (i, p) in pixmap.data().as_chunks::<4>().0.iter().enumerate() {
                let (x, y) = (i % w as usize, i / w as usize);
                let argb = u32::from(p[3]) << 24
                    | u32::from(p[0]) << 16
                    | u32::from(p[1]) << 8
                    | u32::from(p[2]);
                data[y * stride + x * 4..][..4].copy_from_slice(&argb.to_ne_bytes());
            }
        }
        Ok(surface)
    }

    /// The size an image of `w`x`h` is drawn at: its longest side is the
    /// icon size, and neither side is below one pixel.
    fn fitted(&self, w: f64, h: f64) -> (i32, i32) {
        let size = self.size as f64;
        let scale = size / w.max(h).max(f64::MIN_POSITIVE);
        let side = |v: f64| ((v * scale).round() as i32).clamp(1, self.size as i32);
        (side(w), side(h))
    }

    /// `src` scaled once to the icon size, smoothly in both directions.
    fn fit(&self, src: &ImageSurface) -> Result<ImageSurface, String> {
        let (sw, sh) = (src.width() as f64, src.height() as f64);
        let (w, h) = self.fitted(sw, sh);
        let out = ImageSurface::create(Format::ARgb32, w, h).map_err(|e| e.to_string())?;
        let cr = Context::new(&out).map_err(|e| e.to_string())?;
        cr.scale(w as f64 / sw, h as f64 / sh);
        let pattern = SurfacePattern::create(src);
        pattern.set_filter(Filter::Good);
        cr.set_source(&pattern).map_err(|e| e.to_string())?;
        cr.paint().map_err(|e| e.to_string())?;
        drop(cr);
        Ok(out)
    }
}

fn is_svg(path: &Path, bytes: &[u8]) -> bool {
    let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
    ext.eq_ignore_ascii_case("svg") || bytes.trim_ascii_start().starts_with(b"<")
}

/// `gtk-icon-theme-name` from `$XDG_CONFIG_HOME/gtk-3.0/settings.ini`.
fn gtk_icon_theme() -> Option<String> {
    let config = std::env::var_os("XDG_CONFIG_HOME")
        .filter(|v| !v.is_empty())
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".config")))?;
    let text = std::fs::read_to_string(config.join("gtk-3.0/settings.ini")).ok()?;
    text.lines().find_map(|line| {
        let (key, value) = line.split_once('=')?;
        (key.trim() == "gtk-icon-theme-name")
            .then(|| value.trim().trim_matches('"').to_owned())
            .filter(|v| !v.is_empty())
    })
}
