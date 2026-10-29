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

use crate::core::{Button, Notification, Urgency};
use crate::theme::{Theme, ThemeOverride};
use crate::{debug, error, xdg};

/// The fully commented reference config printed by `lucent --default-config`.
/// It parses back to [`Config::default`].
pub const REFERENCE: &str = include_str!("../default_config.toml");

#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
#[serde(default)]
pub struct Config {
    pub general: General,
    pub layout: Layout,
    /// `[theme]`: a Preset (Midnight Bloom if unset) and any Theme field
    /// set on top of it.
    pub theme: ThemeOverride,
    /// `[[rule]]`: per-app or per-Urgency overrides, in file order.
    #[serde(rename = "rule")]
    pub rules: Vec<Rule>,
}

/// `[[rule]]`: overrides for the Notifications it matches. A Rule with no
/// matcher matches every Notification.
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
#[serde(default)]
pub struct Rule {
    /// Matches this app name exactly, ignoring case.
    pub app_name: Option<String>,
    /// Matches this Urgency.
    pub urgency: Option<Urgency>,
    /// How long matching Popups stay up, in milliseconds in the file. It
    /// replaces both `[general] timeout` and the app's request; critical
    /// Notifications still never expire.
    #[serde(deserialize_with = "some_millis")]
    pub timeout: Option<Duration>,
    /// Theme fields (or a Preset) set on top of `[theme]` and earlier Rules.
    pub theme: ThemeOverride,
}

impl Rule {
    /// Whether this Rule applies to `n`, by its app name and Urgency.
    pub fn matches(&self, n: &Notification) -> bool {
        self.app_name
            .as_ref()
            .is_none_or(|name| name.to_lowercase() == n.app_name.to_lowercase())
            && self.urgency.is_none_or(|u| u == n.urgency)
    }
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
    /// Exit once the Stack and Queue stay empty for `idle_grace` (ADR-0002).
    pub idle_exit: bool,
    /// How long an empty Lucent waits before exiting, in milliseconds in the
    /// file.
    #[serde(deserialize_with = "millis")]
    pub idle_grace: Duration,
    /// What each pointer button does on a Popup.
    pub left_click: ClickAction,
    pub middle_click: ClickAction,
    pub right_click: ClickAction,
    /// Whether a sender's requested timeout replaces `timeout`. Critical
    /// Notifications never expire either way.
    #[serde(alias = "honour_app_timeouts")]
    pub honor_app_timeouts: bool,
    /// Body lines shown before the rest is cut with an ellipsis.
    pub body_lines: u32,
}

impl Default for General {
    fn default() -> Self {
        Self {
            timeout: Duration::from_secs(5),
            max_visible: 5,
            idle_exit: true,
            idle_grace: Duration::from_secs(5),
            left_click: ClickAction::InvokeDefaultAction,
            middle_click: ClickAction::Dismiss,
            right_click: ClickAction::DismissAll,
            honor_app_timeouts: true,
            body_lines: 4,
        }
    }
}

impl General {
    /// What a click with `button` does.
    pub fn binding(&self, button: Button) -> ClickAction {
        match button {
            Button::Left => self.left_click,
            Button::Middle => self.middle_click,
            Button::Right => self.right_click,
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
    /// `[theme] preset` names no Preset; Midnight Bloom stands in, with the
    /// `[theme]` overrides still applied.
    UnknownPreset(String),
    /// The `[[rule]]` numbered `rule` (from 1, in file order) names no
    /// Preset; Midnight Bloom stands in for it, with the Rule's own Theme
    /// fields still applied.
    UnknownRulePreset { rule: usize, name: String },
}

impl std::fmt::Display for Problem {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Invalid(e) => write!(f, "invalid config, using the defaults: {e}"),
            Self::UnknownKey(key) => write!(f, "unknown config key `{key}` ignored"),
            Self::UnknownPreset(name) => {
                write!(f, "unknown theme preset `{name}`, using midnight-bloom")?;
                write_presets(f)
            }
            Self::UnknownRulePreset { rule, name } => {
                write!(
                    f,
                    "rule {rule}: unknown theme preset `{name}`, using midnight-bloom"
                )?;
                write_presets(f)
            }
        }
    }
}

/// Lists the Presets after an unknown name, so a typo is easy to fix.
fn write_presets(f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    let known: Vec<_> = Theme::PRESETS.iter().map(|(n, _)| *n).collect();
    write!(f, " (have: {})", known.join(", "))
}

impl Config {
    /// The Theme `[theme]` resolves to, before any Urgency style.
    pub fn theme(&self) -> Theme {
        let mut theme = Theme::default();
        self.theme.apply(&mut theme);
        theme
    }

    /// The Rules matching `n`, in file order, so later ones win.
    pub fn rules_for(&self, n: &Notification) -> impl Iterator<Item = &Rule> {
        self.rules.iter().filter(move |rule| rule.matches(n))
    }

    /// The Theme for `n`: `[theme]`, then each matching Rule, then the
    /// Urgency style.
    pub fn theme_for(&self, n: &Notification) -> Theme {
        let mut theme = self.theme();
        for rule in self.rules_for(n) {
            rule.theme.apply(&mut theme);
        }
        theme.for_urgency(n.urgency)
    }

    /// The timeout the last matching Rule sets, if any does.
    pub fn rule_timeout(&self, n: &Notification) -> Option<Duration> {
        self.rules_for(n).filter_map(|rule| rule.timeout).last()
    }

    /// Resolves config text against the defaults. Never fails: anything
    /// unusable is reported and the defaults stand in.
    pub fn parse(text: &str) -> (Config, Vec<Problem>) {
        let mut unknown = Vec::new();
        let de = match toml::Deserializer::parse(text) {
            Ok(de) => de,
            Err(e) => return (Config::default(), vec![Problem::Invalid(e.to_string())]),
        };
        match serde_ignored::deserialize::<_, _, Config>(de, |path| unknown.push(path.to_string()))
        {
            Ok(config) => {
                let mut problems: Vec<_> = unknown.into_iter().map(Problem::UnknownKey).collect();
                if let Some(name) = config.theme.unknown_preset() {
                    problems.push(Problem::UnknownPreset(name.into()));
                }
                for (i, rule) in config.rules.iter().enumerate() {
                    if let Some(name) = rule.theme.unknown_preset() {
                        problems.push(Problem::UnknownRulePreset {
                            rule: i + 1,
                            name: name.into(),
                        });
                    }
                }
                (config, problems)
            }
            Err(e) => (Config::default(), vec![Problem::Invalid(e.to_string())]),
        }
    }
}

/// `$XDG_CONFIG_HOME/lucent/config.toml`, falling back to `~/.config`.
pub fn path() -> Option<PathBuf> {
    Some(xdg::config_home()?.join("lucent").join("config.toml"))
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

fn some_millis<'de, D: Deserializer<'de>>(de: D) -> Result<Option<Duration>, D::Error> {
    millis(de).map(Some)
}
