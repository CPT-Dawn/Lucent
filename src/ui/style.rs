//! The Popup stylesheet, built from the config and swapped in place when
//! the config reloads.
//!
//! Interim look: the Themes, `[look]` accent and `style.css` arrive with
//! the Popup card and Themes tickets (#4, #5). Until then the colours are
//! Dawn-like constants and only `opacity`, `radius` and `font` apply.

use gtk4::CssProvider;
use gtk4::gdk::Display;
use lucent::config::Config;

/// Dawn-like background, as RGB; the alpha comes from `look.opacity`.
const BACKGROUND_RGB: &str = "13, 11, 20";
const BORDER: &str = "#1A1525";
const TEXT: &str = "#E8E2F0";

fn build_css(config: &Config) -> String {
    let look = &config.look;
    // No font rule means GTK's own setting: the system UI font.
    let font = look
        .font
        .as_deref()
        .map(|font| format!("font-family: \"{}\";", font.replace('"', "\\\"")))
        .unwrap_or_default();

    format!(
        r#"
.lucent-window {{
    background-color: transparent;
    box-shadow: none;
}}

.notification-popup {{
    background-color: rgba({BACKGROUND_RGB}, {opacity});
    border: 1px solid {BORDER};
    border-radius: {radius}px;
    padding: 14px 18px;
    color: {TEXT};
    {font}
}}

.notification-summary {{
    font-weight: 700;
    font-size: 15px;
    line-height: 1.25;
}}

.notification-body {{
    font-size: 13px;
    line-height: 1.3;
    opacity: 0.88;
}}

.notification-app-name {{
    font-size: 11px;
    font-weight: 600;
    opacity: 0.68;
}}
"#,
        opacity = look.opacity,
        radius = look.radius,
    )
}

/// The display-wide stylesheet every Popup window inherits.
pub struct Stylesheet(CssProvider);

impl Stylesheet {
    /// Register the stylesheet for the default display and style it from
    /// `config`.
    pub fn new(config: &Config) -> Self {
        let provider = CssProvider::new();
        if let Some(display) = Display::default() {
            gtk4::style_context_add_provider_for_display(
                &display,
                &provider,
                gtk4::STYLE_PROVIDER_PRIORITY_APPLICATION,
            );
        }
        let stylesheet = Self(provider);
        stylesheet.apply(config);
        stylesheet
    }

    /// Restyle every Popup, including those already on screen.
    pub fn apply(&self, config: &Config) {
        self.0.load_from_string(&build_css(config));
    }
}
