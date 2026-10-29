//! Remembers the last Notification id across Idle exits (ADR-0002).
//!
//! A restarted Lucent must not hand out ids from 1 again, or a client holding
//! an old id could replace or close an unrelated Notification. The id lives
//! in `$XDG_RUNTIME_DIR/lucent/last-id`: tmpfs, private to the user, and
//! gone at logout, when no client can hold an old id any more.

use std::path::PathBuf;

use crate::{debug, error};

pub struct IdStore {
    /// `None` without `$XDG_RUNTIME_DIR`: ids then restart with the process.
    path: Option<PathBuf>,
}

impl IdStore {
    /// The store under `$XDG_RUNTIME_DIR`, if that is set.
    pub fn open() -> Self {
        let path = std::env::var_os("XDG_RUNTIME_DIR")
            .filter(|v| !v.is_empty())
            .map(|dir| PathBuf::from(dir).join("lucent").join("last-id"));
        if path.is_none() {
            debug!("XDG_RUNTIME_DIR is unset; ids will restart at 1");
        }
        Self { path }
    }

    /// The last id handed out by an earlier run, or 0 if there was none.
    pub fn load(&self) -> u32 {
        let Some(path) = &self.path else { return 0 };
        match std::fs::read_to_string(path) {
            Ok(text) => text.trim().parse().unwrap_or_else(|_| {
                error!("ignoring the unreadable id in {}", path.display());
                0
            }),
            Err(_) => 0,
        }
    }

    /// Records `id` as the last one handed out. Written to a temporary file
    /// and renamed, so a crash never leaves a torn number behind.
    pub fn save(&self, id: u32) {
        let Some(path) = &self.path else { return };
        let tmp = path.with_extension("tmp");
        let result = path
            .parent()
            .map_or(Ok(()), std::fs::create_dir_all)
            .and_then(|()| std::fs::write(&tmp, id.to_string()))
            .and_then(|()| std::fs::rename(&tmp, path));
        if let Err(e) = result {
            error!("could not save the last id to {}: {e}", path.display());
        }
    }
}
