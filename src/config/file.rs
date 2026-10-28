//! The config file on disk: where it lives, the template written on first
//! run, and reading it through [`parse`](super::parse).

use std::fmt;
use std::io;
use std::path::{Path, PathBuf};

use super::{Parsed, Rejected, TEMPLATE, parse};

/// Lucent's config directory, `$XDG_CONFIG_HOME/lucent` (usually
/// `~/.config/lucent`).
pub fn dir() -> Option<PathBuf> {
    Some(directories::BaseDirs::new()?.config_dir().join("lucent"))
}

/// The config file, `config.toml` in [`dir`].
pub fn path() -> Option<PathBuf> {
    Some(dir()?.join("config.toml"))
}

/// The name of the optional user stylesheet in [`dir`].
pub const STYLE_CSS: &str = "style.css";

/// The optional user stylesheet, `style.css` next to the config file,
/// applied on top of the Theme.
pub fn style_path(config_path: &Path) -> PathBuf {
    config_path.with_file_name(STYLE_CSS)
}

/// Write the commented template to `path` if there is no file there yet.
/// Returns whether it wrote one.
pub fn write_template_if_missing(path: &Path) -> io::Result<bool> {
    if path.exists() {
        return Ok(false);
    }
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(path, TEMPLATE)?;
    Ok(true)
}

/// Why a config file couldn't be used.
#[derive(Debug)]
pub enum LoadError {
    /// The file exists but couldn't be read.
    Unreadable(io::Error),
    /// The file has errors.
    Invalid(Rejected),
}

impl fmt::Display for LoadError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Unreadable(e) => write!(f, "can't read the config: {e}"),
            Self::Invalid(rejected) => {
                let n = rejected.errors.len();
                write!(
                    f,
                    "the config has {n} error{}",
                    if n == 1 { "" } else { "s" }
                )?;
                for error in &rejected.errors {
                    write!(f, "\n{error}")?;
                }
                Ok(())
            }
        }
    }
}

impl std::error::Error for LoadError {}

/// Read and parse the config at `path`. A missing file gives the defaults.
pub fn load(path: &Path) -> Result<Parsed, LoadError> {
    let text = match std::fs::read_to_string(path) {
        Ok(text) => text,
        Err(e) if e.kind() == io::ErrorKind::NotFound => String::new(),
        Err(e) => return Err(LoadError::Unreadable(e)),
    };
    parse(&text).map_err(LoadError::Invalid)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Config;

    /// A fresh, empty directory for one test.
    fn scratch_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("lucent-test-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn a_missing_file_gives_the_full_defaults() {
        let path = scratch_dir("missing").join("config.toml");
        let parsed = load(&path).unwrap();
        assert_eq!(parsed.config, Config::default());
        assert!(parsed.warnings.is_empty());
    }

    #[test]
    fn first_run_writes_the_template_once_and_it_loads_as_the_defaults() {
        let path = scratch_dir("first-run").join("lucent").join("config.toml");

        assert!(write_template_if_missing(&path).unwrap());
        std::fs::write(&path, "theme = \"dawn\"").unwrap();
        assert!(!write_template_if_missing(&path).unwrap());

        assert_eq!(std::fs::read_to_string(&path).unwrap(), "theme = \"dawn\"");
    }

    #[test]
    fn an_invalid_file_is_reported_with_its_errors() {
        let path = scratch_dir("invalid").join("config.toml");
        std::fs::write(&path, "[popup]\nwidth = \"wide\"\n").unwrap();

        let error = load(&path).unwrap_err();

        assert!(matches!(&error, LoadError::Invalid(r) if r.errors.len() == 1));
        assert!(error.to_string().contains("\n2:9: "), "{error}");
    }
}
