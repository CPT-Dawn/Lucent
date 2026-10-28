//! The `lucent` command line. With no subcommand, `lucent` runs the
//! daemon; each subcommand lives in its own module here.
//!
//! To add a subcommand: add a variant to [`Command`] (its doc comment is
//! the `--help` text), give it a module with a `run` function returning an
//! [`ExitCode`], and dispatch to it in [`Command::run`]. Subcommands that
//! talk to the daemon go through [`daemon::with_daemon`].

mod check;
mod clear;
mod daemon;
mod dismiss;
mod dnd;
mod history;
mod reload;
mod restore;
mod status;

use std::path::PathBuf;
use std::process::ExitCode;

use clap::{Parser, Subcommand};

/// Notifications that don't nag, they glow.
///
/// Run with no subcommand to start the notification daemon (D-Bus also
/// starts it by itself when an app sends a Notification).
#[derive(Debug, Parser)]
#[command(name = "lucent", version, about, long_about)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Option<Command>,
}

#[derive(Debug, Subcommand)]
pub enum Command {
    /// Validate the config and print its warnings and errors.
    ///
    /// Exits non-zero if the config has errors.
    Check {
        /// The config file to check (default: ~/.config/lucent/config.toml).
        path: Option<PathBuf>,
    },
    /// Reload the config in the running daemon now.
    ///
    /// Lucent already reloads whenever the config is saved; this is the
    /// manual fallback. An invalid config keeps the previous one.
    Reload,
    /// Dismiss the newest Popup.
    Dismiss {
        /// Dismiss every Popup, and everything waiting in Overflow.
        #[arg(long)]
        all: bool,
    },
    /// Show a History entry again as a new Popup.
    Restore {
        /// The entry's id, as `lucent history` lists it (default: the most
        /// recent entry).
        id: Option<u32>,
    },
    /// List History, newest first.
    History {
        /// Print JSON for scripts instead of a readable list.
        #[arg(long)]
        json: bool,
    },
    /// Empty History.
    Clear,
    /// Switch Do Not Disturb, or show whether it is on.
    ///
    /// While it is on, Notifications go to History without a Popup, except
    /// Critical ones (unless `allow_critical = false` in `[dnd]`). It is
    /// always off when Lucent starts.
    Dnd {
        /// on, off or toggle (default: just show the current state).
        #[arg(value_enum)]
        change: Option<DndChange>,
    },
    /// Print the History count and Do Not Disturb state as JSON.
    ///
    /// The line is ready for a Waybar custom module with `return-type:
    /// json`: `text` (the count), `alt` and `class` (`none`,
    /// `notification`, `dnd-none` or `dnd-notification`) and `tooltip`,
    /// plus the raw `count` and `dnd`.
    Status {
        /// Keep running and print a new line whenever the status changes.
        #[arg(long)]
        follow: bool,
    },
}

/// How `lucent dnd` changes Do Not Disturb.
#[derive(Debug, Clone, Copy, clap::ValueEnum)]
pub enum DndChange {
    On,
    Off,
    Toggle,
}

impl Command {
    pub fn run(self) -> ExitCode {
        match self {
            Self::Check { path } => check::run(path),
            Self::Reload => reload::run(),
            Self::Dismiss { all } => dismiss::run(all),
            Self::Restore { id } => restore::run(id),
            Self::History { json } => history::run(json),
            Self::Clear => clear::run(),
            Self::Dnd { change } => dnd::run(change),
            Self::Status { follow } => status::run(follow),
        }
    }
}
