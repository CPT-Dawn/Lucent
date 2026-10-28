//! Lucent's configuration: the v2 schema, its forgiving parser and the
//! commented template written on first run.
//!
//! [`parse`] is a pure function from config text to a [`Config`] plus
//! warnings, or to errors. Each [`Diagnostic`] carries a line, a column and
//! a plain-language fix. Every key is optional: an empty file gives the
//! full defaults. `lucent check` and hot-reload both go through [`parse`];
//! [`file`] finds the file on disk and writes the template on first run.

mod diagnostic;
pub mod file;
mod parse;

pub use diagnostic::Diagnostic;
pub use parse::{Parsed, Rejected, parse};

use crate::engine::{DndConfig, EngineConfig, HistoryConfig, Rules, Timeouts};

/// The fully commented config written on first run. Every option is shown
/// with its default, commented out.
pub const TEMPLATE: &str = include_str!("template.toml");

/// Everything a config file can set. `Default` is the out-of-the-box look
/// and behaviour.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Config {
    pub theme: Theme,
    pub popup: PopupConfig,
    /// `[timeout]`: how long Popups stay, by Urgency.
    pub timeouts: Timeouts,
    pub look: LookConfig,
    pub history: HistoryConfig,
    pub dnd: DndConfig,
}

impl Config {
    /// The engine's slice of this config.
    pub fn engine_config(&self) -> EngineConfig {
        EngineConfig {
            max_visible: self.popup.max_visible,
            timeouts: self.timeouts,
            history: self.history.clone(),
            dnd: self.dnd.clone(),
            // `[[rule]]` lands with ticket #11 (Per-app Rules).
            rules: Rules::default(),
        }
    }
}

/// A built-in visual style for Popups.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Theme {
    /// Dawn when the system prefers dark, Daybreak when it prefers light.
    #[default]
    Auto,
    Dawn,
    Daybreak,
    Graphite,
}

impl Theme {
    /// Every name `theme` accepts.
    pub const NAMES: [&str; 4] = ["auto", "dawn", "daybreak", "graphite"];

    pub fn from_name(name: &str) -> Option<Self> {
        Some(match name {
            "auto" => Self::Auto,
            "dawn" => Self::Dawn,
            "daybreak" => Self::Daybreak,
            "graphite" => Self::Graphite,
            _ => return None,
        })
    }
}

/// `[popup]`: where Popups appear and how many.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PopupConfig {
    pub position: Position,
    /// Popup width in pixels.
    pub width: u32,
    /// At most this many Popups are visible; the rest wait in Overflow.
    pub max_visible: usize,
    /// Pixels between stacked Popups.
    pub gap: u32,
    /// Pixels between the stack and the screen edges.
    pub margin: u32,
    /// The output (monitor) to show Popups on; `None` means the focused one.
    pub output: Option<String>,
}

impl Default for PopupConfig {
    fn default() -> Self {
        Self {
            position: Position::default(),
            width: 380,
            max_visible: 5,
            gap: 10,
            margin: 16,
            output: None,
        }
    }
}

/// The screen edge and alignment the Popup stack is anchored to, written
/// `"top-right"`, `"bottom-center"` and so on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Position {
    pub vertical: Vertical,
    pub horizontal: Horizontal,
}

impl Default for Position {
    fn default() -> Self {
        Self {
            vertical: Vertical::Top,
            horizontal: Horizontal::Right,
        }
    }
}

impl Position {
    /// Every name `popup.position` accepts.
    pub const NAMES: [&str; 6] = [
        "top-left",
        "top-center",
        "top-right",
        "bottom-left",
        "bottom-center",
        "bottom-right",
    ];

    pub fn from_name(name: &str) -> Option<Self> {
        let (vertical, horizontal) = name.split_once('-')?;
        Some(Self {
            vertical: match vertical {
                "top" => Vertical::Top,
                "bottom" => Vertical::Bottom,
                _ => return None,
            },
            horizontal: match horizontal {
                "left" => Horizontal::Left,
                "center" => Horizontal::Center,
                "right" => Horizontal::Right,
                _ => return None,
            },
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Vertical {
    Top,
    Bottom,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Horizontal {
    Left,
    Center,
    Right,
}

/// `[look]`: small visual tweaks on top of the Theme.
#[derive(Debug, Clone, PartialEq)]
pub struct LookConfig {
    /// A hex colour replacing the Theme's accent; `None` keeps the Theme's.
    pub accent: Option<String>,
    /// Popup background opacity, from 0.0 to 1.0.
    pub opacity: f64,
    /// Corner radius in pixels.
    pub radius: u32,
    /// Font family; `None` means the system UI font.
    pub font: Option<String>,
    /// Size in pixels of the icon or image inside the 42px icon tile.
    pub icon_size: u32,
    /// Show the countdown line along the bottom of each Popup.
    pub countdown: bool,
}

impl Default for LookConfig {
    fn default() -> Self {
        Self {
            accent: None,
            opacity: 0.9,
            radius: 14,
            font: None,
            icon_size: 40,
            countdown: true,
        }
    }
}
#[cfg(test)]
mod tests;
