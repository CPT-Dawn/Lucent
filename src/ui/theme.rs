//! Themes and the Look: every colour and size a Popup is drawn with.
//!
//! A [`Theme`] is a set of named colour tokens (background, foreground,
//! muted, line, chip, accent-1, accent-2, critical and shadow). The
//! [`Look`] holds the sizes. Both are plain data: [`css`] turns them into
//! the stylesheet, and the cairo-drawn parts (countdown line, and later the
//! progress ring) read the same tokens, so the two never disagree.
//!
//! [`Theme::for_config`] picks the Theme a config asks for, given whether
//! the system prefers dark, and applies the `[look]` accent and opacity.

use std::fmt::Write as _;

use lucent::config::{self, Config, LookConfig, Position};

/// The system's light/dark preference, as far as Lucent can tell.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SystemScheme {
    Dark,
    Light,
    /// The desktop doesn't say (no settings portal, or no preference set).
    NoPreference,
}

/// Whether to use the dark variant. Without a stated preference Lucent
/// stays dark: Dawn is its home Theme.
pub fn prefers_dark(scheme: SystemScheme) -> bool {
    scheme != SystemScheme::Light
}

/// A colour with straight (not premultiplied) alpha.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Rgba {
    pub r: u8,
    pub g: u8,
    pub b: u8,
    pub a: f32,
}

impl Rgba {
    pub const fn new(r: u8, g: u8, b: u8, a: f32) -> Self {
        Self { r, g, b, a }
    }

    /// An opaque colour from `0xRRGGBB`.
    pub const fn hex(rgb: u32) -> Self {
        Self::new((rgb >> 16) as u8, (rgb >> 8) as u8, rgb as u8, 1.0)
    }

    /// A colour from `#rgb`, `#rgba`, `#rrggbb` or `#rrggbbaa`.
    pub fn parse_hex(text: &str) -> Option<Self> {
        let hex = text.strip_prefix('#')?;
        let digit = |i: usize| u8::from_str_radix(hex.get(i..=i)?, 16).ok();
        let pair = |i: usize| u8::from_str_radix(hex.get(i..i + 2)?, 16).ok();
        let (r, g, b, a) = match hex.len() {
            3 | 4 => {
                let short = |i| digit(i).map(|d| d * 17);
                let a = if hex.len() == 4 { short(3)? } else { 255 };
                (short(0)?, short(1)?, short(2)?, a)
            }
            6 | 8 => {
                let a = if hex.len() == 8 { pair(6)? } else { 255 };
                (pair(0)?, pair(2)?, pair(4)?, a)
            }
            _ => return None,
        };
        Some(Self::new(r, g, b, f32::from(a) / 255.0))
    }

    pub fn with_alpha(self, a: f32) -> Self {
        Self { a, ..self }
    }

    /// The colour as GTK CSS, e.g. `rgba(22,17,34,0.76)`.
    pub fn css(self) -> String {
        format!("rgba({},{},{},{})", self.r, self.g, self.b, self.a)
    }

    /// Components in `0.0..=1.0`, as cairo wants them.
    pub fn components(self) -> (f64, f64, f64, f64) {
        (
            f64::from(self.r) / 255.0,
            f64::from(self.g) / 255.0,
            f64::from(self.b) / 255.0,
            f64::from(self.a),
        )
    }

    /// A colour from hue (degrees), saturation and lightness (`0..=1`).
    pub fn hsl(hue: f32, saturation: f32, lightness: f32) -> Self {
        let c = (1.0 - (2.0 * lightness - 1.0).abs()) * saturation;
        let h = hue.rem_euclid(360.0) / 60.0;
        let x = c * (1.0 - (h % 2.0 - 1.0).abs());
        let (r, g, b) = match h as u32 {
            0 => (c, x, 0.0),
            1 => (x, c, 0.0),
            2 => (0.0, c, x),
            3 => (0.0, x, c),
            4 => (x, 0.0, c),
            _ => (c, 0.0, x),
        };
        let m = lightness - c / 2.0;
        let channel = |v: f32| ((v + m) * 255.0).round().clamp(0.0, 255.0) as u8;
        Self::new(channel(r), channel(g), channel(b), 1.0)
    }
}

/// One drop shadow under the card (`box-shadow: 0 y blur spread color`).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Shadow {
    pub y: i32,
    pub blur: i32,
    pub spread: i32,
    pub color: Rgba,
}

impl Shadow {
    fn css(&self) -> String {
        format!(
            "0 {}px {}px {}px {}",
            self.y,
            self.blur,
            self.spread,
            self.color.css()
        )
    }
}

/// Space around a rectangle, in pixels.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Insets {
    pub top: i32,
    pub right: i32,
    pub bottom: i32,
    pub left: i32,
}

/// A named, built-in visual style for Popups.
#[derive(Debug, Clone, PartialEq)]
pub struct Theme {
    pub name: &'static str,
    pub background: Rgba,
    pub foreground: Rgba,
    pub muted: Rgba,
    pub line: Rgba,
    pub chip: Rgba,
    pub accent_1: Rgba,
    pub accent_2: Rgba,
    pub critical: Rgba,
    pub shadow: [Shadow; 2],
    /// The primary Action takes this colour (Graphite uses the foreground).
    pub primary_action: Rgba,
}

impl Theme {
    /// Dark violet glass with a sunrise accent, orange to violet.
    pub fn dawn() -> Self {
        let accent_1 = Rgba::hex(0xff9a62);
        Self {
            name: "dawn",
            background: Rgba::new(22, 17, 34, 0.76),
            foreground: Rgba::hex(0xf2ecfb),
            muted: Rgba::new(242, 236, 251, 0.6),
            line: Rgba::new(255, 255, 255, 0.09),
            chip: Rgba::new(255, 255, 255, 0.08),
            accent_1,
            accent_2: Rgba::hex(0xcf6bff),
            critical: Rgba::hex(0xff5d6c),
            shadow: shadows(-14, Rgba::new(8, 4, 20, 0.65), Rgba::new(8, 4, 20, 0.3)),
            primary_action: accent_1,
        }
    }

    /// Warm frosted light, Dawn's accent family in deeper tones.
    pub fn daybreak() -> Self {
        let accent_1 = Rgba::hex(0xee6b35);
        Self {
            name: "daybreak",
            background: Rgba::new(255, 251, 247, 0.72),
            foreground: Rgba::hex(0x2b2136),
            muted: Rgba::new(43, 33, 54, 0.6),
            line: Rgba::new(43, 33, 54, 0.09),
            chip: Rgba::new(43, 33, 54, 0.06),
            accent_1,
            accent_2: Rgba::hex(0xa648de),
            critical: Rgba::hex(0xe2364a),
            shadow: shadows(
                -16,
                Rgba::new(110, 60, 50, 0.38),
                Rgba::new(110, 60, 50, 0.12),
            ),
            primary_action: accent_1,
        }
    }

    /// Neutral monochrome, dark variant.
    pub fn graphite_dark() -> Self {
        let foreground = Rgba::hex(0xededef);
        Self {
            name: "graphite-dark",
            background: Rgba::new(26, 26, 29, 0.84),
            foreground,
            muted: Rgba::new(237, 237, 239, 0.55),
            line: Rgba::new(255, 255, 255, 0.08),
            chip: Rgba::new(255, 255, 255, 0.07),
            accent_1: Rgba::hex(0xf4f4f5),
            accent_2: Rgba::hex(0x8f8f99),
            critical: Rgba::hex(0xf87171),
            shadow: shadows(-14, Rgba::new(0, 0, 0, 0.7), Rgba::new(0, 0, 0, 0.3)),
            primary_action: foreground,
        }
    }

    /// Neutral monochrome, light variant.
    pub fn graphite_light() -> Self {
        let foreground = Rgba::hex(0x18181b);
        Self {
            name: "graphite-light",
            background: Rgba::new(250, 250, 251, 0.84),
            foreground,
            muted: Rgba::new(24, 24, 27, 0.55),
            line: Rgba::new(24, 24, 27, 0.08),
            chip: Rgba::new(24, 24, 27, 0.06),
            accent_1: Rgba::hex(0x27272a),
            accent_2: Rgba::hex(0x8a8a94),
            critical: Rgba::hex(0xdc2626),
            shadow: shadows(-16, Rgba::new(0, 0, 0, 0.25), Rgba::new(0, 0, 0, 0.08)),
            primary_action: foreground,
        }
    }

    /// The Theme `config` asks for, in its light or dark variant, with the
    /// `[look]` accent and opacity applied.
    pub fn for_config(config: &Config, dark: bool) -> Self {
        let theme = match (config.theme, dark) {
            (config::Theme::Dawn, _) | (config::Theme::Auto, true) => Self::dawn(),
            (config::Theme::Daybreak, _) | (config::Theme::Auto, false) => Self::daybreak(),
            (config::Theme::Graphite, true) => Self::graphite_dark(),
            (config::Theme::Graphite, false) => Self::graphite_light(),
        };
        theme.with_look(&config.look)
    }

    /// `opacity` scales the background's alpha (1.0 keeps the Theme's own
    /// translucency); `accent` replaces both accents, and the primary
    /// Action too where it follows the accent.
    fn with_look(mut self, look: &LookConfig) -> Self {
        let alpha = self.background.a * look.opacity as f32;
        self.background = self.background.with_alpha(alpha);
        if let Some(accent) = look.accent.as_deref().and_then(Rgba::parse_hex) {
            if self.primary_action == self.accent_1 {
                self.primary_action = accent;
            }
            self.accent_1 = accent;
            self.accent_2 = accent;
        }
        self
    }

    /// How far the card's shadows (and the Critical glow) reach beyond it.
    pub fn shadow_reach(&self) -> Insets {
        let glow = CRITICAL_GLOW;
        let mut reach = Insets {
            top: glow,
            right: glow,
            bottom: glow,
            left: glow,
        };
        for s in &self.shadow {
            let around = (s.blur + s.spread).max(0);
            reach.top = reach.top.max(around - s.y);
            reach.bottom = reach.bottom.max(around + s.y);
            reach.left = reach.left.max(around);
            reach.right = reach.right.max(around);
        }
        reach
    }
}

/// The prototype's two-layer card shadow: a wide soft drop and a close one.
fn shadows(spread: i32, far: Rgba, near: Rgba) -> [Shadow; 2] {
    [
        Shadow {
            y: 18,
            blur: 40,
            spread,
            color: far,
        },
        Shadow {
            y: 2,
            blur: 8,
            spread: 0,
            color: near,
        },
    ]
}

/// Spread of the Critical pulse's glow ring, in pixels.
const CRITICAL_GLOW: i32 = 4;

/// Sizes and switches for Popups. `Default` is the spec's defaults.
#[derive(Debug, Clone, PartialEq)]
pub struct Look {
    /// Card width.
    pub width: i32,
    /// Card corner radius.
    pub radius: i32,
    /// The app icon glyph or image inside the tile.
    pub icon_size: i32,
    /// The rounded-square icon tile.
    pub tile: i32,
    /// Whether the countdown line is shown.
    pub countdown: bool,
    /// Gap between stacked Popups.
    pub gap: i32,
    /// Distance from the screen edges.
    pub margin: i32,
    pub position: Position,
    /// Font family; `None` means the system UI font.
    pub font: Option<String>,
}

impl Default for Look {
    fn default() -> Self {
        Self::from_config(&Config::default())
    }
}

impl Look {
    /// The Look a config asks for (`[popup]` and `[look]`). Opacity and
    /// accent belong to the Theme, not here.
    pub fn from_config(config: &Config) -> Self {
        let px = |n: u32| i32::try_from(n).unwrap_or(i32::MAX);
        Self {
            width: px(config.popup.width),
            radius: px(config.look.radius),
            icon_size: px(config.look.icon_size),
            tile: 42,
            countdown: config.look.countdown,
            gap: px(config.popup.gap),
            margin: px(config.popup.margin),
            position: config.popup.position,
            font: config.look.font.clone(),
        }
    }

    /// The `font-family` declaration, if a font is set. Without one GTK
    /// uses the system UI font.
    fn font_rule(&self) -> String {
        self.font
            .as_deref()
            .map(|font| format!("font-family: \"{}\";", font.replace('"', "\\\"")))
            .unwrap_or_default()
    }

    /// Corner radius of the icon tile (12px on the 42px tile).
    pub fn tile_radius(&self) -> f64 {
        f64::from(self.tile) * 12.0 / 42.0
    }

    /// Side of the app badge shown on an image (20px on the 42px tile).
    pub fn badge(&self) -> i32 {
        (f64::from(self.tile) * 20.0 / 42.0).round() as i32
    }

    /// How far the progress ring and badge reach outside the tile.
    pub fn tile_overhang(&self) -> i32 {
        5
    }
}

/// The stylesheet for a Theme and Look.
pub fn css(theme: &Theme, look: &Look) -> String {
    let shadow = theme
        .shadow
        .iter()
        .map(Shadow::css)
        .collect::<Vec<_>>()
        .join(", ");
    let glow = |alpha: f32| {
        format!(
            "{shadow}, 0 0 0 {CRITICAL_GLOW}px {}",
            theme.critical.with_alpha(alpha).css()
        )
    };
    let mut css = String::new();
    let _ = write!(
        css,
        r#"
window.lucent-stack {{ background: none; box-shadow: none; }}

.popup {{
  color: {fg};
  background-color: {bg};
  border: 1px solid {line};
  border-radius: {radius}px;
  box-shadow: {shadow};
  font-size: 13px;
  {font}
}}
.popup label {{ line-height: 1.4; }}
.popup .app {{ font-weight: 600; font-size: 11.5px; color: {muted}; letter-spacing: 0.01em; }}
.popup .time, .popup .pct {{ font-size: 11.5px; color: {muted}; font-feature-settings: "tnum"; }}
.popup .summary {{ font-weight: 650; font-size: 14px; color: {fg}; }}
.popup .body {{ color: {muted}; }}
.popup .body link {{ color: {a1}; }}
.popup .tile.fallback {{ color: #ffffff; font-weight: 700; font-size: 17px; }}
.popup .badge {{ border-radius: 6px; box-shadow: 0 0 0 2px {bg}; }}

.popup button.close {{
  all: unset;
  min-width: 22px;
  min-height: 22px;
  border-radius: 50%;
  background-color: {chip};
  color: {muted};
  opacity: 0;
  transition: opacity 150ms;
}}
.popup button.close image {{ -gtk-icon-size: 12px; }}
.popup:hover button.close {{ opacity: 1; }}

.popup.low {{ opacity: 0.92; }}
.popup.critical {{
  border-color: {crit70};
  box-shadow: {glow_off};
  animation: lucent-pulse 2.4s ease-in-out infinite;
}}
@keyframes lucent-pulse {{
  0% {{ box-shadow: {glow_off}; }}
  50% {{ box-shadow: {glow_on}; }}
  100% {{ box-shadow: {glow_off}; }}
}}
@media (prefers-reduced-motion: reduce) {{
  .popup.critical {{ animation: none; }}
}}
"#,
        fg = theme.foreground.css(),
        bg = theme.background.css(),
        line = theme.line.css(),
        muted = theme.muted.css(),
        chip = theme.chip.css(),
        a1 = theme.accent_1.css(),
        radius = look.radius,
        font = look.font_rule(),
        crit70 = theme.critical.with_alpha(0.7).css(),
        glow_off = glow(0.0),
        glow_on = glow(0.22),
    );
    css.push_str(&action_row_css(theme));
    css.push_str(&pill_css(theme, &shadow));
    css
}

/// The Action row: equal buttons divided by hairlines, the first in the
/// Theme's primary Action colour.
fn action_row_css(theme: &Theme) -> String {
    format!(
        r#"
.popup .actions {{ border-top: 1px solid {line}; }}
.popup .actions > button {{
  all: unset;
  padding: 10px 8px;
  font-size: 12.5px;
  font-weight: 600;
  color: {fg};
  transition: background-color 150ms;
}}
.popup .actions > button > label {{ margin: 0; }}
.popup .actions > button + button {{ border-left: 1px solid {line}; }}
.popup .actions > button.primary {{ color: {primary}; }}
.popup .actions > button:hover {{ background-color: {chip}; }}
"#,
        line = theme.line.css(),
        fg = theme.foreground.css(),
        primary = theme.primary_action.css(),
        chip = theme.chip.css(),
    )
}

/// The "+N more" Overflow pill: a capsule in the card's colours, with
/// three accent-gradient dots.
fn pill_css(theme: &Theme, shadow: &str) -> String {
    format!(
        r#"
button.overflow-pill {{
  all: unset;
  color: {fg};
  background-color: {bg};
  border: 1px solid {line};
  border-radius: 999px;
  box-shadow: {shadow};
  font-weight: 600;
  font-size: 12px;
  padding: 8px 14px;
}}
button.overflow-pill label {{ line-height: 1; }}
button.overflow-pill .dot {{
  min-width: 5px;
  min-height: 5px;
  border-radius: 50%;
  background-image: linear-gradient(90deg, {a1}, {a2});
}}
"#,
        fg = theme.foreground.css(),
        bg = theme.background.css(),
        line = theme.line.css(),
        a1 = theme.accent_1.css(),
        a2 = theme.accent_2.css(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dawn_matches_the_prototype_tokens() {
        let dawn = Theme::dawn();
        assert_eq!(dawn.background.css(), "rgba(22,17,34,0.76)");
        assert_eq!(dawn.foreground.css(), "rgba(242,236,251,1)");
        assert_eq!(dawn.accent_1.css(), "rgba(255,154,98,1)");
        assert_eq!(dawn.accent_2.css(), "rgba(207,107,255,1)");
        assert_eq!(dawn.critical.css(), "rgba(255,93,108,1)");
    }

    #[test]
    fn the_stylesheet_carries_the_theme_and_look() {
        let css = css(&Theme::dawn(), &Look::default());
        assert!(css.contains("background-color: rgba(22,17,34,0.76);"));
        assert!(css.contains("border-radius: 14px;"));
        assert!(css.contains(
            "box-shadow: 0 18px 40px -14px rgba(8,4,20,0.65), 0 2px 8px 0px rgba(8,4,20,0.3);"
        ));
        assert!(css.contains("border-color: rgba(255,93,108,0.7);"));
        assert!(css.contains("0 0 0 4px rgba(255,93,108,0.22)"));
    }

    #[test]
    fn dawns_shadow_reaches_44px_below_and_26px_to_the_sides() {
        assert_eq!(
            Theme::dawn().shadow_reach(),
            Insets {
                top: 8,
                right: 26,
                bottom: 44,
                left: 26,
            }
        );
    }

    #[test]
    fn hsl_converts_known_colours() {
        assert_eq!(Rgba::hsl(0.0, 1.0, 0.5), Rgba::hex(0xff0000));
        assert_eq!(Rgba::hsl(120.0, 1.0, 0.5), Rgba::hex(0x00ff00));
        assert_eq!(Rgba::hsl(240.0, 1.0, 0.25), Rgba::hex(0x000080));
    }

    #[test]
    fn the_look_follows_the_config() {
        let mut config = Config::default();
        config.popup.width = 420;
        config.look.radius = 20;
        config.look.font = Some("Inter".into());

        let look = Look::from_config(&config);

        assert_eq!(look.width, 420);
        assert!(css(&Theme::dawn(), &look).contains("border-radius: 20px;"));
        assert!(css(&Theme::dawn(), &look).contains("font-family: \"Inter\";"));
        assert!(!css(&Theme::dawn(), &Look::default()).contains("font-family"));
    }

    #[test]
    fn tile_geometry_scales_with_the_tile() {
        let look = Look::default();
        assert_eq!(look.tile_radius(), 12.0);
        assert_eq!(look.badge(), 20);
    }

    // ── Choosing a Theme ──

    fn chosen(theme: &str, dark: bool) -> Theme {
        let config = lucent::config::parse(&format!("theme = \"{theme}\""))
            .expect("valid config")
            .config;
        Theme::for_config(&config, dark)
    }

    #[test]
    fn auto_is_dawn_in_the_dark_and_daybreak_in_the_light() {
        assert_eq!(chosen("auto", true).name, "dawn");
        assert_eq!(chosen("auto", false).name, "daybreak");
    }

    #[test]
    fn graphite_follows_the_system_but_dawn_and_daybreak_are_fixed() {
        assert_eq!(chosen("graphite", true).name, "graphite-dark");
        assert_eq!(chosen("graphite", false).name, "graphite-light");
        assert_eq!(chosen("dawn", false).name, "dawn");
        assert_eq!(chosen("daybreak", true).name, "daybreak");
    }

    #[test]
    fn daybreak_matches_the_prototype_tokens() {
        let t = chosen("daybreak", false);
        assert_eq!(t.background.css(), "rgba(255,251,247,0.72)");
        assert_eq!(t.foreground.css(), "rgba(43,33,54,1)");
        assert_eq!(t.muted.css(), "rgba(43,33,54,0.6)");
        assert_eq!(t.line.css(), "rgba(43,33,54,0.09)");
        assert_eq!(t.chip.css(), "rgba(43,33,54,0.06)");
        assert_eq!(t.accent_1.css(), "rgba(238,107,53,1)");
        assert_eq!(t.accent_2.css(), "rgba(166,72,222,1)");
        assert_eq!(t.critical.css(), "rgba(226,54,74,1)");
        assert!(css(&t, &Look::default()).contains(
            "box-shadow: 0 18px 40px -16px rgba(110,60,50,0.38), 0 2px 8px 0px rgba(110,60,50,0.12);"
        ));
    }

    #[test]
    fn graphite_matches_the_prototype_tokens_in_both_variants() {
        let dark = chosen("graphite", true);
        assert_eq!(dark.background.css(), "rgba(26,26,29,0.84)");
        assert_eq!(dark.foreground.css(), "rgba(237,237,239,1)");
        assert_eq!(dark.muted.css(), "rgba(237,237,239,0.55)");
        assert_eq!(dark.line.css(), "rgba(255,255,255,0.08)");
        assert_eq!(dark.chip.css(), "rgba(255,255,255,0.07)");
        assert_eq!(dark.accent_1.css(), "rgba(244,244,245,1)");
        assert_eq!(dark.accent_2.css(), "rgba(143,143,153,1)");
        assert_eq!(dark.critical.css(), "rgba(248,113,113,1)");
        let light = chosen("graphite", false);
        assert_eq!(light.background.css(), "rgba(250,250,251,0.84)");
        assert_eq!(light.foreground.css(), "rgba(24,24,27,1)");
        assert_eq!(light.muted.css(), "rgba(24,24,27,0.55)");
        assert_eq!(light.line.css(), "rgba(24,24,27,0.08)");
        assert_eq!(light.chip.css(), "rgba(24,24,27,0.06)");
        assert_eq!(light.accent_1.css(), "rgba(39,39,42,1)");
        assert_eq!(light.accent_2.css(), "rgba(138,138,148,1)");
        assert_eq!(light.critical.css(), "rgba(220,38,38,1)");
        assert!(css(&light, &Look::default()).contains(
            "box-shadow: 0 18px 40px -16px rgba(0,0,0,0.25), 0 2px 8px 0px rgba(0,0,0,0.08);"
        ));
    }

    #[test]
    fn graphites_primary_action_uses_the_foreground() {
        let graphite = chosen("graphite", true);
        assert_eq!(graphite.primary_action, graphite.foreground);
        let dawn = chosen("dawn", true);
        assert_eq!(dawn.primary_action, dawn.accent_1);
    }

    // ── [look] on top of the Theme ──

    fn with_look(look: &str) -> Theme {
        let config = lucent::config::parse(&format!("theme = \"dawn\"\n[look]\n{look}"))
            .expect("valid config")
            .config;
        Theme::for_config(&config, true)
    }

    #[test]
    fn by_default_each_theme_keeps_its_own_background_alpha() {
        assert_eq!(with_look(""), Theme::dawn());
    }

    #[test]
    fn opacity_scales_the_themes_background_alpha() {
        assert_eq!(
            with_look("opacity = 0.5").background.css(),
            "rgba(22,17,34,0.38)"
        );
        assert_eq!(with_look("opacity = 0").background.a, 0.0);
    }

    #[test]
    fn opacity_leaves_text_and_lines_alone() {
        let faded = with_look("opacity = 0.5");
        assert_eq!(faded.foreground, Theme::dawn().foreground);
        assert_eq!(faded.line, Theme::dawn().line);
    }

    #[test]
    fn an_accent_replaces_both_accent_tokens_and_the_primary_action() {
        let t = with_look("accent = \"#3d8aff\"");
        assert_eq!(t.accent_1.css(), "rgba(61,138,255,1)");
        assert_eq!(t.accent_2, t.accent_1);
        assert_eq!(t.primary_action, t.accent_1);
        assert!(css(&t, &Look::default()).contains("link { color: rgba(61,138,255,1); }"));
    }

    #[test]
    fn short_and_translucent_accents_are_understood() {
        assert_eq!(
            with_look("accent = \"#f80\"").accent_1.css(),
            "rgba(255,136,0,1)"
        );
        assert_eq!(
            with_look("accent = \"#ff880080\"").accent_1.css(),
            "rgba(255,136,0,0.5019608)"
        );
    }

    #[test]
    fn graphite_keeps_its_foreground_primary_action_under_an_accent() {
        let config = lucent::config::parse("theme = \"graphite\"\n[look]\naccent = \"#3d8aff\"")
            .expect("valid config")
            .config;
        let t = Theme::for_config(&config, true);
        assert_eq!(t.accent_1.css(), "rgba(61,138,255,1)");
        assert_eq!(t.primary_action, t.foreground);
    }

    #[test]
    fn the_system_prefers_dark_unless_it_says_light() {
        assert!(prefers_dark(SystemScheme::Dark));
        assert!(!prefers_dark(SystemScheme::Light));
        assert!(prefers_dark(SystemScheme::NoPreference));
    }
}
