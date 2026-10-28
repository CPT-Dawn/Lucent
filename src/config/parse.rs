//! The forgiving config parser: TOML text in, [`Config`] plus warnings or
//! errors out. It walks the parsed document by hand, rather than through
//! serde, so every warning and error can point at the exact key or value.

use std::ops::RangeInclusive;
use std::time::Duration;

use toml::Spanned;
use toml::de::{DeTable, DeValue};

use super::{Config, Diagnostic, Position, Theme};

/// A config that parsed, with the warnings worth showing the user.
#[derive(Debug, Clone, PartialEq)]
pub struct Parsed {
    pub config: Config,
    pub warnings: Vec<Diagnostic>,
}

/// A config that can't be used, with everything wrong with it.
#[derive(Debug, Clone, PartialEq)]
pub struct Rejected {
    pub errors: Vec<Diagnostic>,
    pub warnings: Vec<Diagnostic>,
}

/// Parse config text. Missing keys keep their defaults, unknown and v1
/// keys warn, and invalid values are errors.
pub fn parse(text: &str) -> Result<Parsed, Rejected> {
    let document = match DeTable::parse(text) {
        Ok(document) => document,
        Err(e) => {
            let offset = e.span().map_or(0, |span| span.start);
            let error = Diagnostic::at(
                text,
                offset,
                format!("this isn't valid TOML: {}", e.message().trim()),
                Some(
                    "check the syntax on this line: text needs quotes (`theme = \"dawn\"`), \
                     and each key goes on its own line, once per section"
                        .into(),
                ),
            );
            return Err(Rejected {
                errors: vec![error],
                warnings: Vec::new(),
            });
        }
    };

    let mut parser = Parser {
        text,
        config: Config::default(),
        warnings: Vec::new(),
        errors: Vec::new(),
    };
    parser.root(document.get_ref());
    parser.warnings.sort_by_key(|d| (d.line, d.column));
    parser.errors.sort_by_key(|d| (d.line, d.column));

    if parser.errors.is_empty() {
        Ok(Parsed {
            config: parser.config,
            warnings: parser.warnings,
        })
    } else {
        Err(Rejected {
            errors: parser.errors,
            warnings: parser.warnings,
        })
    }
}

type Key<'i> = Spanned<std::borrow::Cow<'i, str>>;
type Value<'i> = Spanned<DeValue<'i>>;

/// Top-level keys of the v2 schema, for "did you mean" suggestions.
const ROOT_KEYS: &[&str] = &[
    "theme", "popup", "timeout", "look", "history", "dnd", "rule",
];
const POPUP_KEYS: &[&str] = &[
    "position",
    "width",
    "max_visible",
    "gap",
    "margin",
    "output",
];
const TIMEOUT_KEYS: &[&str] = &["low", "normal", "critical"];
const LOOK_KEYS: &[&str] = &[
    "accent",
    "opacity",
    "radius",
    "font",
    "icon_size",
    "countdown",
];
const HISTORY_KEYS: &[&str] = &["limit"];
const DND_KEYS: &[&str] = &["allow_critical"];

/// v1 keys and where their job went in v2.
const V1_KEYS: &[(&str, &str)] = &[
    ("width", "`width` under `[popup]`"),
    (
        "timeout_seconds",
        "`low`, `normal` and `critical` under `[timeout]`",
    ),
    (
        "background_color",
        "`theme` (dawn, daybreak or graphite) or `style.css`",
    ),
    ("border_color", "`theme` or `style.css`"),
    ("border_width", "`style.css`"),
    ("text_color", "`theme` or `style.css`"),
    ("corner_radius", "`radius` under `[look]`"),
    ("font_family", "`font` under `[look]`"),
    ("max_visible_notifications", "`max_visible` under `[popup]`"),
];

/// Longest timeout accepted, in seconds (a day).
const MAX_TIMEOUT: f64 = 86_400.0;

struct Parser<'t> {
    text: &'t str,
    config: Config,
    warnings: Vec<Diagnostic>,
    errors: Vec<Diagnostic>,
}

impl Parser<'_> {
    fn root(&mut self, table: &DeTable<'_>) {
        for (key, value) in table {
            match key.get_ref().as_ref() {
                "theme" => {
                    if let Some(theme) =
                        self.choice("theme", value, &Theme::NAMES, Theme::from_name)
                    {
                        self.config.theme = theme;
                    }
                }
                "popup" => self.section(key, value, |p, k, v| p.popup(k, v)),
                "timeout" => self.section(key, value, |p, k, v| p.timeout(k, v)),
                "look" => self.section(key, value, |p, k, v| p.look(k, v)),
                "history" => self.section(key, value, |p, k, v| p.history(k, v)),
                "dnd" => self.section(key, value, |p, k, v| p.dnd(k, v)),
                // `[[rule]]` is parsed by ticket #11 (Per-app Rules).
                "rule" => {}
                name => match V1_KEYS.iter().find(|(v1, _)| *v1 == name) {
                    Some((_, replacement)) => self.warn(
                        key.span().start,
                        format!("`{name}` is a Lucent v1 key and is ignored; use {replacement}"),
                        None,
                    ),
                    None => self.unknown("", key, ROOT_KEYS),
                },
            }
        }
    }

    /// Walk a `[section]`, handing each key to `entry`.
    fn section(
        &mut self,
        key: &Key<'_>,
        value: &Value<'_>,
        entry: impl Fn(&mut Self, &Key<'_>, &Value<'_>),
    ) {
        let name = key.get_ref();
        match value.get_ref() {
            DeValue::Table(table) => {
                for (k, v) in table {
                    entry(self, k, v);
                }
            }
            other => self.error(
                value,
                format!("`{name}` must be a section, but this is {}", kind(other)),
                format!("start the section with a `[{name}]` line and put its keys below it"),
            ),
        }
    }

    fn popup(&mut self, key: &Key<'_>, value: &Value<'_>) {
        let popup = &mut self.config.popup;
        match key.get_ref().as_ref() {
            "position" => {
                if let Some(position) = self.choice(
                    "popup.position",
                    value,
                    &Position::NAMES,
                    Position::from_name,
                ) {
                    self.config.popup.position = position;
                }
            }
            "width" => {
                let default = popup.width;
                if let Some(width) = self.pixels("width", value, 120..=4000, default) {
                    self.config.popup.width = width;
                }
            }
            "max_visible" => {
                let default = popup.max_visible as u32;
                if let Some(max) = self.integer("max_visible", value, 1..=50, default, "Popups") {
                    self.config.popup.max_visible = max as usize;
                }
            }
            "gap" => {
                let default = popup.gap;
                if let Some(gap) = self.pixels("gap", value, 0..=500, default) {
                    self.config.popup.gap = gap;
                }
            }
            "margin" => {
                let default = popup.margin;
                if let Some(margin) = self.pixels("margin", value, 0..=2000, default) {
                    self.config.popup.margin = margin;
                }
            }
            "output" => {
                if let Some(output) = self.string("output", value, "\"DP-1\"") {
                    self.config.popup.output = non_empty(output);
                }
            }
            _ => self.unknown("popup", key, POPUP_KEYS),
        }
    }

    fn timeout(&mut self, key: &Key<'_>, value: &Value<'_>) {
        let name = key.get_ref().as_ref();
        let slot = match name {
            "low" => &mut self.config.timeouts.low,
            "normal" => &mut self.config.timeouts.normal,
            "critical" => &mut self.config.timeouts.critical,
            _ => return self.unknown("timeout", key, TIMEOUT_KEYS),
        };
        let default = slot.map_or(0.0, |d| d.as_secs_f64());
        if let Some(seconds) = self.number(name, value, 0.0..=MAX_TIMEOUT, default, "seconds") {
            let timeout = (seconds > 0.0).then(|| Duration::from_secs_f64(seconds));
            match name {
                "low" => self.config.timeouts.low = timeout,
                "normal" => self.config.timeouts.normal = timeout,
                _ => self.config.timeouts.critical = timeout,
            }
        }
    }

    fn look(&mut self, key: &Key<'_>, value: &Value<'_>) {
        let look = &self.config.look;
        match key.get_ref().as_ref() {
            "accent" => {
                if let Some(accent) = self.string("accent", value, "\"#ff8a3d\"") {
                    if accent.is_empty() || is_hex_colour(&accent) {
                        self.config.look.accent = non_empty(accent);
                    } else {
                        self.error(
                            value,
                            format!("`accent` must be a hex colour, but `{accent}` isn't one"),
                            "write it as `#rrggbb` (or `#rgb`, `#rrggbbaa`), e.g. \
                             `accent = \"#ff8a3d\"`, or `\"\"` for the Theme's own"
                                .into(),
                        );
                    }
                }
            }
            "opacity" => {
                let default = look.opacity;
                if let Some(opacity) = self.number("opacity", value, 0.0..=1.0, default, "") {
                    self.config.look.opacity = opacity;
                }
            }
            "radius" => {
                let default = look.radius;
                if let Some(radius) = self.pixels("radius", value, 0..=200, default) {
                    self.config.look.radius = radius;
                }
            }
            "font" => {
                if let Some(font) = self.string("font", value, "\"Inter\"") {
                    self.config.look.font = non_empty(font);
                }
            }
            "icon_size" => {
                let default = look.icon_size;
                if let Some(size) = self.pixels("icon_size", value, 8..=256, default) {
                    self.config.look.icon_size = size;
                }
            }
            "countdown" => {
                if let Some(countdown) = self.boolean("countdown", value) {
                    self.config.look.countdown = countdown;
                }
            }
            _ => self.unknown("look", key, LOOK_KEYS),
        }
    }

    fn history(&mut self, key: &Key<'_>, value: &Value<'_>) {
        match key.get_ref().as_ref() {
            "limit" => {
                let default = self.config.history.limit as u32;
                if let Some(limit) = self.integer("limit", value, 0..=10_000, default, "entries") {
                    self.config.history.limit = limit as usize;
                }
            }
            _ => self.unknown("history", key, HISTORY_KEYS),
        }
    }

    fn dnd(&mut self, key: &Key<'_>, value: &Value<'_>) {
        match key.get_ref().as_ref() {
            "allow_critical" => {
                if let Some(allow) = self.boolean("allow_critical", value) {
                    self.config.dnd.allow_critical = allow;
                }
            }
            _ => self.unknown("dnd", key, DND_KEYS),
        }
    }

    // ── Value readers: each reports its own error and returns `None` ──

    fn pixels(
        &mut self,
        name: &str,
        value: &Value<'_>,
        range: RangeInclusive<u32>,
        default: u32,
    ) -> Option<u32> {
        self.integer(name, value, range, default, "pixels")
    }

    fn integer(
        &mut self,
        name: &str,
        value: &Value<'_>,
        range: RangeInclusive<u32>,
        default: u32,
        unit: &str,
    ) -> Option<u32> {
        let example = format!("e.g. `{name} = {default}` (the default)");
        let parsed = match value.get_ref() {
            DeValue::Integer(i) => i64::from_str_radix(i.as_str(), i.radix()).ok(),
            other => {
                self.error(
                    value,
                    format!(
                        "`{name}` must be a whole number of {unit}, but this is {}",
                        kind(other)
                    ),
                    format!("write a number without quotes, {example}"),
                );
                return None;
            }
        };
        match parsed.and_then(|n| u32::try_from(n).ok()) {
            Some(n) if range.contains(&n) => Some(n),
            _ => {
                self.error(
                    value,
                    format!(
                        "`{name}` must be between {} and {} {unit}",
                        range.start(),
                        range.end()
                    ),
                    format!("pick a value in that range, {example}"),
                );
                None
            }
        }
    }

    /// A whole or fractional number; `unit` may be empty.
    fn number(
        &mut self,
        name: &str,
        value: &Value<'_>,
        range: RangeInclusive<f64>,
        default: f64,
        unit: &str,
    ) -> Option<f64> {
        let example = format!("e.g. `{name} = {default}` (the default)");
        let unit_suffix = if unit.is_empty() {
            String::new()
        } else {
            format!(" {unit}")
        };
        let parsed = match value.get_ref() {
            DeValue::Integer(i) => i64::from_str_radix(i.as_str(), i.radix())
                .ok()
                .map(|n| n as f64),
            DeValue::Float(f) => f.as_str().parse::<f64>().ok(),
            other => {
                self.error(
                    value,
                    format!(
                        "`{name}` must be a number{}, but this is {}",
                        if unit.is_empty() {
                            String::new()
                        } else {
                            format!(" of {unit}")
                        },
                        kind(other)
                    ),
                    format!("write a number without quotes, {example}"),
                );
                return None;
            }
        };
        match parsed {
            Some(n) if range.contains(&n) => Some(n),
            _ => {
                self.error(
                    value,
                    format!(
                        "`{name}` must be between {} and {}{unit_suffix}",
                        range.start(),
                        range.end()
                    ),
                    format!("pick a value in that range, {example}"),
                );
                None
            }
        }
    }

    fn boolean(&mut self, name: &str, value: &Value<'_>) -> Option<bool> {
        match value.get_ref() {
            DeValue::Boolean(b) => Some(*b),
            other => {
                self.error(
                    value,
                    format!(
                        "`{name}` must be true or false, but this is {}",
                        kind(other)
                    ),
                    format!("write `{name} = true` or `{name} = false`, without quotes"),
                );
                None
            }
        }
    }

    fn string(&mut self, name: &str, value: &Value<'_>, example: &str) -> Option<String> {
        match value.get_ref() {
            DeValue::String(s) => Some(s.to_string()),
            other => {
                self.error(
                    value,
                    format!(
                        "`{name}` must be text in quotes, but this is {}",
                        kind(other)
                    ),
                    format!("put the value in double quotes, e.g. `{name} = {example}`"),
                );
                None
            }
        }
    }

    /// One of a fixed set of names.
    fn choice<T>(
        &mut self,
        name: &str,
        value: &Value<'_>,
        names: &[&str],
        from_name: impl Fn(&str) -> Option<T>,
    ) -> Option<T> {
        let short = name.rsplit('.').next().unwrap_or(name);
        let list = names
            .iter()
            .map(|n| format!("\"{n}\""))
            .collect::<Vec<_>>()
            .join(", ");
        let text = self.string(short, value, &format!("\"{}\"", names[0]))?;
        if let Some(choice) = from_name(&text.to_lowercase()) {
            return Some(choice);
        }
        let help = match closest(&text.to_lowercase(), names) {
            Some(near) => format!("did you mean `{short} = \"{near}\"`? It can be one of {list}"),
            None => format!("use one of {list}"),
        };
        self.error(value, format!("`{name}` can't be \"{text}\""), help);
        None
    }

    // ── Diagnostics ──

    fn unknown(&mut self, section: &str, key: &Key<'_>, known: &[&str]) {
        let name = key.get_ref();
        let full = if section.is_empty() {
            format!("`{name}`")
        } else {
            format!("`{name}` in `[{section}]`")
        };
        let help = closest(name, known).map(|near| format!("did you mean `{near}`?"));
        self.warn(
            key.span().start,
            format!("unknown key {full} is ignored"),
            help,
        );
    }

    fn warn(&mut self, offset: usize, message: String, help: Option<String>) {
        self.warnings
            .push(Diagnostic::at(self.text, offset, message, help));
    }

    fn error(&mut self, value: &Value<'_>, message: String, help: String) {
        self.errors.push(Diagnostic::at(
            self.text,
            value.span().start,
            message,
            Some(help),
        ));
    }
}

/// How a TOML value reads in an error message.
fn kind(value: &DeValue<'_>) -> &'static str {
    match value {
        DeValue::String(_) => "text",
        DeValue::Integer(_) => "a whole number",
        DeValue::Float(_) => "a decimal number",
        DeValue::Boolean(_) => "true/false",
        DeValue::Datetime(_) => "a date",
        DeValue::Array(_) => "a list",
        DeValue::Table(_) => "a section",
    }
}

fn non_empty(s: String) -> Option<String> {
    (!s.trim().is_empty()).then_some(s)
}

fn is_hex_colour(s: &str) -> bool {
    s.strip_prefix('#').is_some_and(|hex| {
        matches!(hex.len(), 3 | 4 | 6 | 8) && hex.chars().all(|c| c.is_ascii_hexdigit())
    })
}

/// The known name closest to a typo, if any is close enough.
fn closest<'k>(typo: &str, known: &[&'k str]) -> Option<&'k str> {
    known
        .iter()
        .map(|k| (edit_distance(typo, k), *k))
        .filter(|(d, k)| *d <= if k.len() > 3 { 2 } else { 1 })
        .min_by_key(|(d, _)| *d)
        .map(|(_, k)| k)
}

fn edit_distance(a: &str, b: &str) -> usize {
    let b: Vec<char> = b.chars().collect();
    let mut row: Vec<usize> = (0..=b.len()).collect();
    for (i, ca) in a.chars().enumerate() {
        let mut prev = row[0];
        row[0] = i + 1;
        for (j, cb) in b.iter().enumerate() {
            let cur = row[j + 1];
            row[j + 1] = (prev + usize::from(ca != *cb)).min(row[j] + 1).min(cur + 1);
            prev = cur;
        }
    }
    row[b.len()]
}
