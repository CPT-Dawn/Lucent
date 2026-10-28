//! Hot-reload: watches the config file and applies each valid save live.
//! An invalid save keeps the last good config and logs what's wrong. The
//! user's `style.css` next to it is watched too, and restyles on its own.
//!
//! The watch is a GIO file monitor (inotify), so it costs nothing until
//! the file changes. Editors often save in several steps (write a temp
//! file, rename it over the original), so changes are coalesced for a
//! moment before reloading.

use std::cell::RefCell;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::time::Duration;

use gtk4::gio::{self, prelude::*};
use gtk4::glib::{self, SourceId};
use lucent::config::{Config, Diagnostic, Parsed, file};

/// How long to wait for an editor to finish saving before reloading.
const SETTLE: Duration = Duration::from_millis(150);

/// The config the daemon runs with, kept in step with the file on disk.
pub struct LiveConfig {
    path: PathBuf,
    apply: Box<dyn Fn(&Config)>,
    restyle: Option<Box<dyn Fn()>>,
    pending: RefCell<Option<SourceId>>,
    pending_style: RefCell<Option<SourceId>>,
    monitor: RefCell<Option<gio::FileMonitor>>,
}

impl LiveConfig {
    /// Load the config at `path`, writing the commented template first if
    /// there is no file yet. An invalid config is logged and the defaults
    /// are used, so a typo never leaves the user without notifications.
    pub fn load(path: PathBuf) -> (Config, LiveConfigBuilder) {
        match file::write_template_if_missing(&path) {
            Ok(true) => tracing::info!("wrote a commented config to {}", path.display()),
            Ok(false) => {}
            Err(e) => tracing::warn!("couldn't write a config to {}: {e}", path.display()),
        }
        let config = match file::load(&path) {
            Ok(parsed) => accept(&path, parsed),
            Err(e) => {
                log_rejection(&path, &e);
                tracing::warn!("using the default config until it's fixed");
                Config::default()
            }
        };
        (
            config,
            LiveConfigBuilder {
                path,
                restyle: None,
            },
        )
    }

    /// Reload now. On success the new config is applied and its warnings
    /// returned; on failure the previous config stays.
    pub fn reload(&self) -> Result<Vec<String>, String> {
        match file::load(&self.path) {
            Ok(parsed) => {
                let warnings = parsed.warnings.iter().map(ToString::to_string).collect();
                let config = accept(&self.path, parsed);
                (self.apply)(&config);
                tracing::info!("reloaded {}", self.path.display());
                Ok(warnings)
            }
            Err(e) => {
                log_rejection(&self.path, &e);
                tracing::warn!("keeping the previous config");
                Err(e.to_string())
            }
        }
    }

    /// Reload whenever the file is saved.
    fn watch(self: &Rc<Self>) {
        let Some(dir) = self.path.parent() else {
            return;
        };
        // Watch the directory, not the file: editors that save by renaming
        // a new file into place would otherwise detach the watch.
        let monitor = match gio::File::for_path(dir)
            .monitor_directory(gio::FileMonitorFlags::WATCH_MOVES, gio::Cancellable::NONE)
        {
            Ok(monitor) => monitor,
            Err(e) => {
                tracing::warn!("can't watch {} for changes: {e}", dir.display());
                return;
            }
        };
        let name = self.path.file_name().map(ToOwned::to_owned);
        let style = Some(std::ffi::OsString::from(file::STYLE_CSS));
        let this = Rc::downgrade(self);
        monitor.connect_changed(move |_, file, other, _| {
            let Some(this) = this.upgrade() else {
                return;
            };
            let touches = |wanted: &Option<std::ffi::OsString>| {
                [Some(file), other]
                    .into_iter()
                    .flatten()
                    .any(|f| f.basename().map(|b| b.into_os_string()) == *wanted)
            };
            if touches(&name) {
                this.reload_soon();
            }
            if touches(&style) {
                this.restyle_soon();
            }
        });
        *self.monitor.borrow_mut() = Some(monitor);
    }

    /// Reload once the file has been quiet for [`SETTLE`].
    fn reload_soon(self: &Rc<Self>) {
        self.settle(
            |this| &this.pending,
            |this| {
                let _ = this.reload();
            },
        );
    }

    /// Reapply `style.css` once it has been quiet for [`SETTLE`].
    fn restyle_soon(self: &Rc<Self>) {
        self.settle(
            |this| &this.pending_style,
            |this| {
                if let Some(restyle) = &this.restyle {
                    restyle();
                }
            },
        );
    }

    /// Run `then` once nothing has called this for [`SETTLE`]; `pending`
    /// picks the slot that remembers the waiting run.
    fn settle(self: &Rc<Self>, pending: fn(&Self) -> &RefCell<Option<SourceId>>, then: fn(&Self)) {
        if let Some(source) = pending(self).borrow_mut().take() {
            source.remove();
        }
        let this = Rc::downgrade(self);
        let source = glib::timeout_add_local_once(SETTLE, move || {
            if let Some(this) = this.upgrade() {
                pending(&this).borrow_mut().take();
                then(&this);
            }
        });
        *pending(self).borrow_mut() = Some(source);
    }
}

/// Finishes a [`LiveConfig`] once the daemon knows how to apply a config.
pub struct LiveConfigBuilder {
    path: PathBuf,
    restyle: Option<Box<dyn Fn()>>,
}

impl LiveConfigBuilder {
    /// Run `restyle` whenever `style.css` in the config directory is saved.
    pub fn on_style_change(mut self, restyle: impl Fn() + 'static) -> Self {
        self.restyle = Some(Box::new(restyle));
        self
    }

    /// Start watching; `apply` runs with every valid reload.
    pub fn watch(self, apply: impl Fn(&Config) + 'static) -> Rc<LiveConfig> {
        let live = Rc::new(LiveConfig {
            path: self.path,
            apply: Box::new(apply),
            restyle: self.restyle,
            pending: RefCell::default(),
            pending_style: RefCell::default(),
            monitor: RefCell::default(),
        });
        live.watch();
        live
    }
}

fn accept(path: &Path, parsed: Parsed) -> Config {
    for warning in &parsed.warnings {
        tracing::warn!("{}:{}", path.display(), one_line(warning));
    }
    parsed.config
}

fn log_rejection(path: &Path, error: &file::LoadError) {
    match error {
        file::LoadError::Invalid(rejected) => {
            for warning in &rejected.warnings {
                tracing::warn!("{}:{}", path.display(), one_line(warning));
            }
            for error in &rejected.errors {
                tracing::error!("{}:{}", path.display(), one_line(error));
            }
        }
        file::LoadError::Unreadable(e) => {
            tracing::error!("can't read {}: {e}", path.display());
        }
    }
}

/// A diagnostic on one log line.
fn one_line(diagnostic: &Diagnostic) -> String {
    match &diagnostic.help {
        Some(help) => format!(
            "{}:{}: {} ({help})",
            diagnostic.line, diagnostic.column, diagnostic.message
        ),
        None => format!(
            "{}:{}: {}",
            diagnostic.line, diagnostic.column, diagnostic.message
        ),
    }
}
