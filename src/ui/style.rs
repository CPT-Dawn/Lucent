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
    let border_rule = if config.border_width == 0 {
        "border: none;".to_string()
    } else {
        format!(
            "border: {}px solid {};",
            config.border_width, config.border_color
        )
    };

    format!(
        r#"
.lucent-window {{
    background-color: transparent;
    box-shadow: none;
}}

.notification-popup {{
    background-color: {bg};
    {border_rule}
    border-radius: {radius}px;
    padding: 14px 18px;
    color: {text};
}}

.notification-summary {{
    font-weight: 700;
    font-size: 15px;
    line-height: 1.25;
    font-family: "{font}";
    color: {text};
}}

.notification-body {{
    font-size: 13px;
    line-height: 1.3;
    font-family: "{font}";
    color: {text};
    opacity: 0.88;
}}

.notification-app-name {{
    font-size: 11px;
    font-weight: 600;
    font-family: "{font}";
    color: {text};
    opacity: 0.68;
}}
"#,
        bg = config.background_color,
        border_rule = border_rule,
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
