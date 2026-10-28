//! What goes in a Popup's icon tile. An image (`image-data` or
//! `image-path`) takes the tile and the app icon becomes a corner badge;
//! otherwise the app icon (`app_icon`, then `desktop-entry`) fills it.
//! Each can be an icon theme name, a file path or a `file://` URI.
//! Turning these into textures is GTK's job (see `card.rs`); this module
//! only decides.

use std::path::PathBuf;

use lucent::engine::{Image, ImageData, Notification};

/// Where an icon or image comes from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Source {
    /// A name looked up in the icon theme.
    Themed(String),
    /// A file on disk.
    File(PathBuf),
    /// A `file://` URI.
    Uri(String),
}

impl Source {
    /// Interpret an `app_icon` / `image-path` string; empty means none.
    pub fn parse(text: &str) -> Option<Source> {
        let text = text.trim();
        if text.is_empty() {
            None
        } else if text.starts_with("file://") {
            Some(Source::Uri(text.to_owned()))
        } else if text.starts_with('/') || text.starts_with("~/") {
            Some(Source::File(expand_home(text)))
        } else {
            Some(Source::Themed(text.to_owned()))
        }
    }
}

fn expand_home(path: &str) -> PathBuf {
    match (path.strip_prefix("~/"), std::env::var_os("HOME")) {
        (Some(rest), Some(home)) => PathBuf::from(home).join(rest),
        _ => PathBuf::from(path),
    }
}

/// The image that takes the whole tile.
#[derive(Debug, Clone, PartialEq)]
pub enum Art<'a> {
    Source(Source),
    Pixels(&'a ImageData),
}

/// What the tile shows.
#[derive(Debug, Clone, PartialEq)]
pub struct Plan<'a> {
    /// An image that fills the tile; the app icon is then a badge.
    pub art: Option<Art<'a>>,
    /// App icon candidates, best first. The first one that resolves is
    /// used; if none does and there is no art, the tile shows a lettered
    /// fallback.
    pub icons: Vec<Source>,
}

pub fn plan(notification: &Notification) -> Plan<'_> {
    let art = match &notification.hints.image {
        Some(Image::Data(data)) if pixel_layout(data).is_some() => Some(Art::Pixels(data)),
        Some(Image::Path(path)) => Source::parse(&path.to_string_lossy()).map(Art::Source),
        _ => None,
    };
    let icons = [
        Some(notification.app_icon.as_str()),
        notification.hints.desktop_entry.as_deref(),
    ]
    .into_iter()
    .flatten()
    .filter_map(Source::parse)
    .collect();
    Plan { art, icons }
}

/// Whether `image-data` pixels are usable: 8-bit RGB or RGBA with enough
/// bytes for every row. Returns whether they have alpha.
pub fn pixel_layout(data: &ImageData) -> Option<bool> {
    let channels = if data.has_alpha { 4 } else { 3 };
    let (width, height, stride) = (
        usize::try_from(data.width).ok()?,
        usize::try_from(data.height).ok()?,
        usize::try_from(data.rowstride).ok()?,
    );
    let row = width.checked_mul(channels)?;
    let needed = stride
        .checked_mul(height.checked_sub(1)?)?
        .checked_add(row)?;
    (data.bits_per_sample == 8
        && data.channels == channels as i32
        && width > 0
        && stride >= row
        && data.data.len() >= needed)
        .then_some(data.has_alpha)
}

/// The letter shown on the fallback tile.
pub fn initial(app_name: &str) -> String {
    app_name
        .chars()
        .find(|c| c.is_alphanumeric())
        .map(|c| c.to_uppercase().collect())
        .unwrap_or_else(|| "•".to_owned())
}

/// A stable hue (degrees) for an app's fallback tile.
pub fn hue(app_name: &str) -> f32 {
    let hash = app_name.bytes().fold(0x811c_9dc5_u32, |h, b| {
        (h ^ u32::from(b)).wrapping_mul(0x0100_0193)
    });
    (hash % 360) as f32
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::*;
    use lucent::engine::{Expiry, Hints};

    fn notification(app_icon: &str, image: Option<Image>) -> Notification {
        Notification {
            id: 1,
            app_name: "Firefox".into(),
            app_icon: app_icon.into(),
            summary: String::new(),
            body: String::new(),
            actions: Vec::new(),
            hints: Hints {
                image,
                desktop_entry: Some("org.mozilla.firefox".into()),
                ..Default::default()
            },
            expiry: Expiry::Default,
        }
    }

    fn pixels(width: i32, height: i32, has_alpha: bool, len: usize) -> ImageData {
        let channels = if has_alpha { 4 } else { 3 };
        ImageData {
            width,
            height,
            rowstride: width * channels,
            has_alpha,
            bits_per_sample: 8,
            channels,
            data: Arc::from(vec![0u8; len]),
        }
    }

    #[test]
    fn icon_strings_can_be_names_paths_or_uris() {
        assert_eq!(Source::parse(""), None);
        assert_eq!(
            Source::parse("firefox"),
            Some(Source::Themed("firefox".into()))
        );
        assert_eq!(
            Source::parse("/usr/share/pixmaps/x.png"),
            Some(Source::File("/usr/share/pixmaps/x.png".into()))
        );
        assert_eq!(
            Source::parse("file:///tmp/a%20b.png"),
            Some(Source::Uri("file:///tmp/a%20b.png".into()))
        );
    }

    #[test]
    fn without_an_image_the_app_icon_fills_the_tile() {
        let n = notification("firefox", None);
        let plan = plan(&n);
        assert_eq!(plan.art, None);
        assert_eq!(
            plan.icons,
            [
                Source::Themed("firefox".into()),
                Source::Themed("org.mozilla.firefox".into())
            ]
        );
    }

    #[test]
    fn an_image_takes_the_tile_and_the_app_icon_becomes_the_badge() {
        let n = notification("spotify", Some(Image::Path("/tmp/cover.jpg".into())));
        let plan = plan(&n);
        assert_eq!(
            plan.art,
            Some(Art::Source(Source::File("/tmp/cover.jpg".into())))
        );
        assert_eq!(plan.icons[0], Source::Themed("spotify".into()));
    }

    #[test]
    fn image_data_is_used_when_its_pixels_add_up() {
        let good = notification("", Some(Image::Data(pixels(2, 2, true, 16))));
        let short = notification("", Some(Image::Data(pixels(2, 2, true, 15))));
        assert!(matches!(plan(&good).art, Some(Art::Pixels(_))));
        assert_eq!(plan(&short).art, None);
    }

    #[test]
    fn pixel_layout_rejects_inconsistent_data() {
        assert_eq!(pixel_layout(&pixels(2, 1, false, 6)), Some(false));
        let mut wrong_channels = pixels(2, 1, false, 6);
        wrong_channels.channels = 4;
        assert_eq!(pixel_layout(&wrong_channels), None);
        assert_eq!(pixel_layout(&pixels(0, 1, true, 0)), None);
        assert_eq!(pixel_layout(&pixels(1, 0, true, 0)), None);
        let mut sixteen_bit = pixels(1, 1, true, 4);
        sixteen_bit.bits_per_sample = 16;
        assert_eq!(pixel_layout(&sixteen_bit), None);
    }

    #[test]
    fn the_fallback_tile_shows_the_apps_initial_in_a_stable_hue() {
        assert_eq!(initial("firefox"), "F");
        assert_eq!(initial("  "), "•");
        assert_eq!(hue("firefox"), hue("firefox"));
        assert!(hue("firefox") < 360.0);
    }
}
