//! The Popup stylesheet: the Theme's generated CSS plus the user's
//! optional `style.css` on top, both swapped in place when they change.
//!
//! The Theme follows the config and, for `auto` and Graphite, the system's
//! light/dark preference, which GTK reads from the settings portal and
//! reports live. Whoever draws with the Theme's tokens directly (the
//! cairo-drawn parts) hears about each switch through
//! [`Stylesheet::connect_theme_changed`].

use std::cell::RefCell;
use std::path::PathBuf;
use std::rc::Rc;

use gtk4::gdk::Display;
use gtk4::{CssProvider, InterfaceColorScheme, Settings};
use lucent::config::Config;

use super::theme::{self, Look, SystemScheme, Theme, prefers_dark};

/// The display-wide stylesheet every Popup inherits.
pub struct Stylesheet(Rc<Inner>);

/// Hears about each Theme switch.
type ThemeListener = Box<dyn Fn(&Theme)>;

struct Inner {
    /// The Theme and Look, at application priority.
    theme_css: CssProvider,
    /// The user's `style.css`, at user priority so it wins over the Theme.
    user_css: CssProvider,
    user_path: PathBuf,
    config: RefCell<Config>,
    theme: RefCell<Theme>,
    on_theme: RefCell<Vec<ThemeListener>>,
}

impl Stylesheet {
    /// Register the stylesheet for the default display, style it from
    /// `config` and the user's `style.css` at `user_path` (if it exists),
    /// and follow the system's light/dark preference.
    pub fn new(config: &Config, user_path: PathBuf) -> Self {
        let theme_css = provider("theme stylesheet");
        let user_css = provider("style.css");
        if let Some(display) = Display::default() {
            gtk4::style_context_add_provider_for_display(
                &display,
                &theme_css,
                gtk4::STYLE_PROVIDER_PRIORITY_APPLICATION,
            );
            gtk4::style_context_add_provider_for_display(
                &display,
                &user_css,
                gtk4::STYLE_PROVIDER_PRIORITY_USER,
            );
        }
        let inner = Rc::new(Inner {
            theme_css,
            user_css,
            user_path,
            config: RefCell::new(config.clone()),
            theme: RefCell::new(Theme::for_config(config, system_prefers_dark())),
            on_theme: RefCell::default(),
        });
        if let Some(settings) = Settings::default() {
            let weak = Rc::downgrade(&inner);
            settings.connect_gtk_interface_color_scheme_notify(move |_| {
                if let Some(inner) = weak.upgrade() {
                    tracing::info!(dark = system_prefers_dark(), "system colour scheme changed");
                    inner.restyle();
                }
            });
        }
        inner.restyle();
        inner.reload_user_css();
        Self(inner)
    }

    /// The Theme in use now.
    pub fn theme(&self) -> Theme {
        self.0.theme.borrow().clone()
    }

    /// Run `f` with the new Theme whenever it changes.
    pub fn connect_theme_changed(&self, f: impl Fn(&Theme) + 'static) {
        self.0.on_theme.borrow_mut().push(Box::new(f));
    }

    /// Restyle every Popup for a new config, including those on screen.
    pub fn apply(&self, config: &Config) {
        *self.0.config.borrow_mut() = config.clone();
        self.0.restyle();
    }

    /// Reread `style.css`. A missing file means no user CSS; a CSS error
    /// is logged and the rest of the file (and the Theme) still applies.
    pub fn reload_user_css(&self) {
        self.0.reload_user_css();
    }
}

impl Inner {
    fn restyle(&self) {
        let config = self.config.borrow();
        let theme = Theme::for_config(&config, system_prefers_dark());
        self.theme_css
            .load_from_string(&theme::css(&theme, &Look::from_config(&config)));
        let changed = *self.theme.borrow() != theme;
        *self.theme.borrow_mut() = theme;
        if changed {
            let theme = self.theme.borrow();
            tracing::info!(theme = theme.name, "Theme changed");
            for f in self.on_theme.borrow().iter() {
                f(&theme);
            }
        }
    }

    fn reload_user_css(&self) {
        let path = &self.user_path;
        match std::fs::read_to_string(path) {
            Ok(css) => {
                tracing::info!("applying {}", path.display());
                self.user_css.load_from_string(&css);
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                self.user_css.load_from_string("");
            }
            Err(e) => {
                tracing::warn!("can't read {}: {e}", path.display());
                self.user_css.load_from_string("");
            }
        }
    }
}

/// A CSS provider that logs its parse errors under `name`.
fn provider(name: &'static str) -> CssProvider {
    let provider = CssProvider::new();
    provider.connect_parsing_error(move |_, section, error| {
        tracing::warn!("{name} {}: {error}", section.to_str());
    });
    provider
}

fn system_prefers_dark() -> bool {
    let scheme = Settings::default().map(|s| s.gtk_interface_color_scheme());
    prefers_dark(match scheme {
        Some(InterfaceColorScheme::Dark) => SystemScheme::Dark,
        Some(InterfaceColorScheme::Light) => SystemScheme::Light,
        _ => SystemScheme::NoPreference,
    })
}
