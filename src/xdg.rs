//! The XDG base directories Lucent reads from.

use std::path::PathBuf;

/// `$XDG_CONFIG_HOME`, falling back to `~/.config`.
pub fn config_home() -> Option<PathBuf> {
    base("XDG_CONFIG_HOME", ".config")
}

/// `$XDG_DATA_HOME`, falling back to `~/.local/share`.
pub fn data_home() -> Option<PathBuf> {
    base("XDG_DATA_HOME", ".local/share")
}

/// `var` if it is set and not empty (as the spec says), else `fallback` under
/// `$HOME`.
fn base(var: &str, fallback: &str) -> Option<PathBuf> {
    std::env::var_os(var)
        .filter(|v| !v.is_empty())
        .map(PathBuf::from)
        .or_else(|| {
            let home = std::env::var_os("HOME").filter(|v| !v.is_empty())?;
            Some(PathBuf::from(home).join(fallback))
        })
}
