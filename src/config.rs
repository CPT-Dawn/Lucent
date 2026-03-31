//! Configuration bootloader: embed → detect → write → parse.
//!
//! On first launch, if `~/.config/lucent/config.toml` doesn't exist, the
//! embedded default is written to disk so the user always has a template
//! to customise.

use serde::Deserialize;
use std::path::PathBuf;

/// The beautifully commented default configuration, baked into the binary
/// at compile time via `include_str!`.
const DEFAULT_CONFIG: &str = include_str!("../default_config.toml");

/// Strongly-typed configuration parsed from the user's TOML file.
#[derive(Debug, Clone, Deserialize)]
#[allow(dead_code)] // Fields consumed in Phase 4 UI rendering.
pub struct Config {
    /// Width of the notification popup in pixels.
    pub width: u32,
    /// Seconds before a notification auto-dismisses.
    pub timeout_seconds: u32,
    /// Background color (hex, e.g. `"#0D0B14E6"`).
    pub background_color: String,
    /// Border color (hex).
    pub border_color: String,
    /// Border width in pixels (`0` disables the border entirely).
    #[serde(default = "default_border_width")]
    pub border_width: u32,
    /// Text color (hex).
    pub text_color: String,
    /// Corner radius in pixels.
    pub corner_radius: u32,
    /// Font family name.
    pub font_family: String,
    /// Maximum notifications visible at once; extras are queued.
    pub max_visible_notifications: u32,
}

fn default_border_width() -> u32 {
    1
}

/// Resolve the config file path: `$XDG_CONFIG_HOME/lucent/config.toml`
/// (typically `~/.config/lucent/config.toml`).
fn config_path() -> Option<PathBuf> {
    let dirs = directories::BaseDirs::new()?;
    Some(dirs.config_dir().join("lucent").join("config.toml"))
}

/// Load the user config from disk, bootstrapping the default file first
/// if the directory or file doesn't exist yet.
pub fn load_or_create_config() -> Result<Config, Box<dyn std::error::Error>> {
    let path = config_path().ok_or("Could not determine XDG config directory")?;

    // Bootstrap: create the directory tree and write the embedded default.
    if !path.exists() {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(&path, DEFAULT_CONFIG)?;
        eprintln!("[lucent] Wrote default config to {}", path.display());
    }

    let contents = std::fs::read_to_string(&path)?;
    let config: Config = toml::from_str(&contents)?;

    Ok(config)
}
