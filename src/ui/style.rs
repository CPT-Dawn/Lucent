//! GTK4 CSS theming — translucent backgrounds, rounded corners,
//! dawn-hued accents, all driven by the user's `Config`.
//!
//! A single global `CssProvider` is built from config values and loaded
//! once at startup so every notification window inherits the theme.

use gtk4::gdk::Display;
use gtk4::CssProvider;

use crate::config::Config;

/// Build the CSS stylesheet from user configuration values.
fn build_css(config: &Config) -> String {
    format!(
        r#"
.notification-popup {{
    background-color: {bg};
    border: 1px solid {border};
    border-radius: {radius}px;
    padding: 12px 16px;
    color: {text};
}}

.notification-summary {{
    font-weight: bold;
    font-size: 14px;
    font-family: "{font}";
    color: {text};
}}

.notification-body {{
    font-size: 12px;
    font-family: "{font}";
    color: {text};
    opacity: 0.85;
}}

.notification-app-name {{
    font-size: 10px;
    font-family: "{font}";
    color: {text};
    opacity: 0.6;
}}
"#,
        bg = config.background_color,
        border = config.border_color,
        text = config.text_color,
        radius = config.corner_radius,
        font = config.font_family,
    )
}

/// Load the global CSS provider for all notification windows.
pub fn load_css(config: &Config) {
    let css = build_css(config);
    let provider = CssProvider::new();
    provider.load_from_string(&css);

    if let Some(display) = Display::default() {
        gtk4::style_context_add_provider_for_display(
            &display,
            &provider,
            gtk4::STYLE_PROVIDER_PRIORITY_APPLICATION,
        );
    }
}
