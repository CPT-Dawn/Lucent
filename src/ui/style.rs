//! The Popup stylesheet, built from the Theme and Look and swapped in
//! place when the config reloads.
//!
//! The Theme is Dawn for now; the Themes ticket (#5) picks it from
//! `theme` and applies the `[look]` accent and opacity.

use gtk4::CssProvider;
use gtk4::gdk::Display;
use lucent::config::Config;

use super::theme::{self, Look, Theme};

/// The display-wide stylesheet every Popup inherits.
pub struct Stylesheet(CssProvider);

impl Stylesheet {
    /// Register the stylesheet for the default display and style it from
    /// `config`.
    pub fn new(config: &Config) -> Self {
        let provider = CssProvider::new();
        provider.connect_parsing_error(|_, section, error| {
            tracing::warn!("stylesheet {}: {error}", section.to_str());
        });
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
        self.0
            .load_from_string(&theme::css(&Theme::dawn(), &Look::from_config(config)));
    }
}
