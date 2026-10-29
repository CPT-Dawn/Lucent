//! The Theme: every visual setting a Popup is drawn with, and the Presets
//! that fill it in.
//!
//! A Theme is resolved in layers, each overriding only the fields it sets:
//! the chosen Preset, then the `[theme]` overrides, then each matching Rule,
//! then the Urgency style. [`ThemeOverride`] is that "only the fields it
//! sets" layer, shared by `[theme]` and every Rule.
//!
//! Lucent draws no shadows or glows (ADR-0001): there are no fields for them.

use serde::{Deserialize, Deserializer, de};

use crate::core::Urgency;

/// A colour with straight (not premultiplied) alpha, 0–255 per channel.
///
/// In the config it is written `"#RRGGBB"`, `"#RRGGBBAA"`, `"rgb(r, g, b)"`
/// or `"rgba(r, g, b, a)"` with `a` from 0.0 to 1.0.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Rgba {
    pub r: u8,
    pub g: u8,
    pub b: u8,
    pub a: u8,
}

impl Rgba {
    pub const fn new(r: u8, g: u8, b: u8, a: u8) -> Self {
        Self { r, g, b, a }
    }

    /// `0xRRGGBB` at `alpha` (0.0–1.0), for writing Presets like the spec.
    const fn hex(rgb: u32, alpha: f64) -> Self {
        Self::new(
            (rgb >> 16) as u8,
            (rgb >> 8) as u8,
            rgb as u8,
            alpha_byte(alpha),
        )
    }

    /// The same colour with its alpha scaled by `percent`.
    pub fn faded(self, percent: u8) -> Self {
        let a = (u16::from(self.a) * u16::from(percent.min(100)) + 50) / 100;
        Self { a: a as u8, ..self }
    }

    /// This colour with `percent` of `tint` mixed in, keeping its alpha.
    pub fn tinted(self, tint: Self, percent: u8) -> Self {
        let p = u16::from(percent.min(100));
        let mix = |a: u8, b: u8| ((u16::from(a) * (100 - p) + u16::from(b) * p + 50) / 100) as u8;
        Self {
            r: mix(self.r, tint.r),
            g: mix(self.g, tint.g),
            b: mix(self.b, tint.b),
            a: self.a,
        }
    }

    /// Channels as Cairo wants them, 0.0–1.0.
    pub fn to_f64(self) -> (f64, f64, f64, f64) {
        let f = |c: u8| f64::from(c) / 255.0;
        (f(self.r), f(self.g), f(self.b), f(self.a))
    }

    fn parse(text: &str) -> Option<Self> {
        let text = text.trim();
        if let Some(hex) = text.strip_prefix('#') {
            if !hex.is_ascii() {
                return None;
            }
            let byte = |i: usize| u8::from_str_radix(hex.get(i..i + 2)?, 16).ok();
            return match hex.len() {
                6 => Some(Self::new(byte(0)?, byte(2)?, byte(4)?, 255)),
                8 => Some(Self::new(byte(0)?, byte(2)?, byte(4)?, byte(6)?)),
                _ => None,
            };
        }
        let (args, has_alpha) = if let Some(rest) = text.strip_prefix("rgba(") {
            (rest.strip_suffix(')')?, true)
        } else {
            (text.strip_prefix("rgb(")?.strip_suffix(')')?, false)
        };
        let parts: Vec<&str> = args.split(',').map(str::trim).collect();
        let channel = |i: usize| parts.get(i)?.parse::<u8>().ok();
        let alpha = match (has_alpha, parts.len()) {
            (true, 4) => {
                let a: f64 = parts[3].parse().ok()?;
                (0.0..=1.0).contains(&a).then(|| alpha_byte(a))?
            }
            (false, 3) => 255,
            _ => return None,
        };
        Some(Self::new(channel(0)?, channel(1)?, channel(2)?, alpha))
    }
}

const fn alpha_byte(alpha: f64) -> u8 {
    (alpha * 255.0 + 0.5) as u8
}

impl<'de> Deserialize<'de> for Rgba {
    fn deserialize<D: Deserializer<'de>>(de: D) -> Result<Self, D::Error> {
        let text = String::deserialize(de)?;
        Self::parse(&text).ok_or_else(|| {
            de::Error::custom(format!(
                "`{text}` is not a colour; use \"#RRGGBB\", \"#RRGGBBAA\" or \"rgba(r, g, b, a)\""
            ))
        })
    }
}

/// A Preset: its name in the config, and the Theme it makes.
pub type Preset = (&'static str, fn() -> Theme);

/// Declares the Theme's fields once and derives from that list both
/// [`Theme`] and [`ThemeOverride`], so a new field can never be missing from
/// the override layer.
macro_rules! theme_fields {
    ($( $(#[doc = $doc:literal])* $field:ident: $ty:ty, )*) => {
        /// The complete set of visual settings for one Popup.
        #[derive(Debug, Clone, PartialEq, Eq)]
        pub struct Theme {
            $( $(#[doc = $doc])* pub $field: $ty, )*
        }

        /// A layer on top of a Theme: optionally a Preset to start again
        /// from, then any Theme fields, each `None` unless set. `[theme]` is
        /// one; a Rule can carry another.
        #[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
        #[serde(default)]
        pub struct ThemeOverride {
            /// Replace the whole Theme with this Preset before setting the
            /// fields. A name that matches no Preset counts as Midnight Bloom.
            pub preset: Option<String>,
            $( $(#[doc = $doc])* pub $field: Option<$ty>, )*
        }

        impl ThemeOverride {
            /// Changes `theme` by this layer: the Preset first, then each field set.
            pub fn apply(&self, theme: &mut Theme) {
                if let Some(name) = self.preset.as_deref() {
                    *theme = Theme::preset(name).unwrap_or_default();
                }
                $( if let Some(value) = &self.$field {
                    theme.$field = value.clone();
                } )*
            }

            /// The Preset name, if it names none, so it can be reported.
            pub fn unknown_preset(&self) -> Option<&str> {
                self.preset
                    .as_deref()
                    .filter(|name| Theme::preset(name).is_none())
            }
        }

        impl Theme {
            /// Every field's name, as written in the config.
            pub const FIELDS: &[&str] = &[$( stringify!($field), )*];
        }
    };
}

theme_fields! {
    /// Pango font family list for all text.
    font: String,
    /// App name size, in points.
    app_name_size: u32,
    /// Summary size, in points; the summary is semibold.
    summary_size: u32,
    /// Body size, in points.
    body_size: u32,
    /// Fill behind the text.
    background: Rgba,
    /// The outline.
    border: Rgba,
    /// The outline's top edge, a little brighter to catch the light.
    border_top: Rgba,
    /// Outline thickness, in pixels; 0 draws none.
    border_width: u32,
    /// Corner radius, in pixels.
    radius: u32,
    /// Space between the edge and the text, in pixels.
    padding: u32,
    app_name: Rgba,
    summary: Rgba,
    body: Rgba,
    /// The accent line beside the text, and other highlights.
    accent: Rgba,
    /// Width of the accent line, in pixels; 0 draws none.
    accent_width: u32,
    /// The progress bar's empty track.
    progress_track: Rgba,
    /// The progress bar's filled part.
    progress_fill: Rgba,
    /// Border, accent and progress fill of critical Notifications.
    critical: Rgba,
    /// How visible low-urgency text, accent and progress fill are, in percent.
    low_opacity: u8,
    /// The longest side of a Popup's icon or image, in pixels.
    icon_size: u32,
    /// The freedesktop icon theme app icons are looked up in; empty means
    /// the GTK icon theme, else `hicolor`.
    icon_theme: String,
}

impl Theme {
    /// The default Preset: dark smoked glass with mint accents, made to sit
    /// under compositor blur yet stay readable without it. Text is one
    /// neutral hue at three strengths, so weight and opacity carry the
    /// hierarchy, and mint is kept for the app name and highlights.
    pub fn midnight_bloom() -> Self {
        Self {
            font: "Inter Variable, Inter, sans-serif".into(),
            app_name_size: 9,
            summary_size: 10,
            body_size: 10,
            background: Rgba::hex(0x151C20, 0.80),
            border: Rgba::new(255, 255, 255, alpha_byte(0.08)),
            border_top: Rgba::new(255, 255, 255, alpha_byte(0.20)),
            border_width: 1,
            radius: 18,
            padding: 14,
            app_name: Rgba::new(141, 227, 220, alpha_byte(0.78)),
            summary: Rgba::hex(0xEEF4F3, 1.0),
            body: Rgba::new(226, 240, 238, alpha_byte(0.74)),
            accent: Rgba::hex(0x8DE3DC, 1.0),
            accent_width: 0,
            progress_track: Rgba::new(255, 255, 255, alpha_byte(0.12)),
            progress_fill: Rgba::hex(0x8DE3DC, 1.0),
            critical: Rgba::hex(0xF7768E, 1.0),
            low_opacity: 70,
            icon_size: 40,
            icon_theme: String::new(),
        }
    }

    /// Paper-white glass with ink text and a teal accent, for light desktops.
    pub fn light() -> Self {
        Self {
            background: Rgba::hex(0xF6F7F8, 0.86),
            border: Rgba::new(10, 20, 25, alpha_byte(0.10)),
            border_top: Rgba::new(255, 255, 255, alpha_byte(0.90)),
            app_name: Rgba::hex(0x118C74, 0.85),
            summary: Rgba::hex(0x14191C, 1.0),
            body: Rgba::new(16, 24, 28, alpha_byte(0.72)),
            accent: Rgba::hex(0x118C74, 1.0),
            progress_track: Rgba::new(10, 20, 25, alpha_byte(0.10)),
            progress_fill: Rgba::hex(0x118C74, 1.0),
            critical: Rgba::hex(0xF52A65, 1.0),
            ..Self::midnight_bloom()
        }
    }

    /// Midnight Bloom on solid ground, for compositors without blur.
    pub fn opaque() -> Self {
        Self {
            background: Rgba::hex(0x1A2226, 1.0),
            border: Rgba::new(252, 253, 253, alpha_byte(0.10)),
            border_top: Rgba::new(252, 253, 253, alpha_byte(0.20)),
            ..Self::midnight_bloom()
        }
    }

    /// The Presets, by the name `[theme] preset` uses.
    pub const PRESETS: &[Preset] = &[
        ("midnight-bloom", Self::midnight_bloom),
        ("light", Self::light),
        ("opaque", Self::opaque),
    ];

    /// The Preset called `name`, if there is one.
    pub fn preset(name: &str) -> Option<Self> {
        Self::PRESETS
            .iter()
            .find(|(n, _)| *n == name)
            .map(|(_, make)| make())
    }

    /// This Theme with every size cut to what a Popup can draw, so no
    /// config, however odd, can overflow a layout or a buffer.
    pub fn bounded(mut self) -> Self {
        for size in [
            &mut self.app_name_size,
            &mut self.summary_size,
            &mut self.body_size,
        ] {
            *size = (*size).clamp(1, 96);
        }
        self.border_width = self.border_width.min(64);
        self.accent_width = self.accent_width.min(64);
        self.padding = self.padding.min(256);
        self.radius = self.radius.min(1024);
        self.icon_size = self.icon_size.clamp(1, crate::icon::MAX_SIZE);
        self.low_opacity = self.low_opacity.min(100);
        self
    }

    /// This Theme as worn by a Notification of `urgency`.
    /// Critical takes a wash of the `critical` colour in its background, a
    /// softened `critical` border, and `critical` accent and progress fill;
    /// low fades its text, accent and progress fill to
    /// `low_opacity`.
    pub fn for_urgency(mut self, urgency: Urgency) -> Self {
        match urgency {
            Urgency::Low => {
                let percent = self.low_opacity;
                for colour in [
                    &mut self.app_name,
                    &mut self.summary,
                    &mut self.body,
                    &mut self.accent,
                    &mut self.progress_fill,
                ] {
                    *colour = colour.faded(percent);
                }
            }
            Urgency::Normal => {}
            Urgency::Critical => {
                self.background = self.background.tinted(self.critical, 7);
                self.border = self.critical.faded(55);
                self.border_top = self.critical.faded(85);
                self.accent = self.critical;
                self.progress_fill = self.critical;
            }
        }
        self
    }
}

impl Default for Theme {
    fn default() -> Self {
        Self::midnight_bloom()
    }
}
