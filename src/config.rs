//! User configuration: built-in defaults overlaid with an optional, partial
//! TOML file.
//!
//! The file is read once per start (idle exit makes live reload pointless)
//! and Lucent never writes one. [`Config::parse`] is pure and reports what it
//! could not use as [`Problem`]s; [`load`] adds the file and the logging.
//!
//! Each `[section]` is its own struct with `#[serde(default)]`, so a field or
//! a whole section left out keeps its built-in default. A new section (e.g.
//! `[theme]`, or `[[rule]]` as a `Vec`) is one more field on [`Config`].

use std::path::PathBuf;
use std::time::Duration;

use serde::{Deserialize, Deserializer};

use crate::{debug, error};

/// The fully commented reference config printed by `lucent --default-config`.
/// It parses back to [`Config::default`].
pub const REFERENCE: &str = include_str!("../default_config.toml");

#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
#[serde(default)]
pub struct Config {
    pub general: General,
    pub layout: Layout,
}

/// `[general]`
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(default)]
pub struct General {
    /// How long a Popup stays up, in milliseconds in the file.
    #[serde(deserialize_with = "millis")]
    pub timeout: Duration,
    /// The most Popups in the Stack; the rest wait in the Queue.
    pub max_visible: usize,
    /// What each pointer button does on a Popup.
    pub left_click: ClickAction,
    pub middle_click: ClickAction,
    pub right_click: ClickAction,
}

impl Default for General {
    fn default() -> Self {
        Self {
            timeout: Duration::from_secs(5),
            max_visible: 5,
            left_click: ClickAction::InvokeDefaultAction,
            middle_click: ClickAction::Dismiss,
            right_click: ClickAction::DismissAll,
        }
    }
}

/// What a click on a Popup does, bound per button in `[general]`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ClickAction {
    /// Run the Default action, if the Notification has one, then dismiss.
    InvokeDefaultAction,
    /// Dismiss this Popup without running anything.
    Dismiss,
    /// Dismiss every Popup, and the Queue behind them.
    DismissAll,
    /// Do nothing.
    None,
}

/// `[layout]`
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(default)]
pub struct Layout {
    pub anchor: Anchor,
    pub margin: Margin,
    /// Popup width in pixels.
    pub width: u32,
    /// Space between Popups in the Stack, in pixels.
    pub gap: u32,
    /// The output name (e.g. `eDP-1`); `None` lets the compositor choose.
    pub monitor: Option<String>,
}

impl Default for Layout {
    fn default() -> Self {
        Self {
            anchor: Anchor::TopRight,
            margin: Margin::default(),
            width: 360,
            gap: 8,
            monitor: None,
        }
    }
}

/// The screen corner or edge the Stack starts from and grows away from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Anchor {
    TopLeft,
    #[serde(alias = "top-centre")]
    TopCenter,
    TopRight,
    BottomLeft,
    #[serde(alias = "bottom-centre")]
    BottomCenter,
    BottomRight,
}

impl Anchor {
    pub fn is_top(self) -> bool {
        matches!(self, Self::TopLeft | Self::TopCenter | Self::TopRight)
    }
}

/// `[layout.margin]`: distance from each screen edge, in pixels. Only the
/// edges the anchor touches have an effect.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(default)]
pub struct Margin {
    pub top: u32,
    pub right: u32,
    pub bottom: u32,
    pub left: u32,
}

impl Default for Margin {
    fn default() -> Self {
        Self {
            top: 16,
            right: 16,
            bottom: 16,
            left: 16,
        }
    }
}

/// Something in the config text Lucent could not use.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Problem {
    /// Not valid TOML, or a value of the wrong type: the whole file is
    /// ignored in favour of the defaults.
    Invalid(String),
    /// A key Lucent does not know, as a dotted path; it is skipped.
    UnknownKey(String),
}

impl std::fmt::Display for Problem {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Invalid(e) => write!(f, "invalid config, using the defaults: {e}"),
            Self::UnknownKey(key) => write!(f, "unknown config key `{key}` ignored"),
        }
    }
}

impl Config {
    /// Resolves config text against the defaults. Never fails: anything
    /// unusable is reported and the defaults stand in.
    pub fn parse(text: &str) -> (Config, Vec<Problem>) {
        let mut unknown = Vec::new();
        let de = match toml::Deserializer::parse(text) {
            Ok(de) => de,
            Err(e) => return (Config::default(), vec![Problem::Invalid(e.to_string())]),
        };
        match serde_ignored::deserialize(de, |path| unknown.push(path.to_string())) {
            Ok(config) => (
                config,
                unknown.into_iter().map(Problem::UnknownKey).collect(),
            ),
            Err(e) => (Config::default(), vec![Problem::Invalid(e.to_string())]),
        }
    }
}

/// `$XDG_CONFIG_HOME/lucent/config.toml`, falling back to `~/.config`.
pub fn path() -> Option<PathBuf> {
    let base = std::env::var_os("XDG_CONFIG_HOME")
        .filter(|v| !v.is_empty())
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".config")))?;
    Some(base.join("lucent").join("config.toml"))
}

/// Reads the user's config, logging every problem. A missing file is not a
/// problem; an unreadable or broken one falls back to the defaults.
pub fn load() -> Config {
    let Some(path) = path() else {
        return Config::default();
    };
    let text = match std::fs::read_to_string(&path) {
        Ok(text) => text,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            debug!("no config at {}; using the defaults", path.display());
            return Config::default();
        }
        Err(e) => {
            error!("could not read {}, using the defaults: {e}", path.display());
            return Config::default();
        }
    };
    let (config, problems) = Config::parse(&text);
    for problem in problems {
        error!("{}: {problem}", path.display());
    }
    config
}

fn millis<'de, D: Deserializer<'de>>(de: D) -> Result<Duration, D::Error> {
    u64::deserialize(de).map(Duration::from_millis)
}
