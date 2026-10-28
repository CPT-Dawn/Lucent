//! Lucent's private control interface, `io.github.cpt_dawn.Lucent`, used
//! by the `lucent` subcommands.
//!
//! The daemon serves [`ControlServer`]; the CLI talks to it through
//! [`ControlProxy`]. Every method hands a [`Request`] to the GTK main loop,
//! which owns the engine, and awaits the answer. To add a command:
//!
//! 1. add a method to [`ControlServer`] and the [`Control`] proxy trait;
//! 2. add a [`Command`] variant if the engine answers it (most do), or a
//!    [`Request`] variant if the main loop answers it itself;
//! 3. answer it in `crate::engine_host::Core::answer` (or in `daemon.rs`).

use std::time::{Instant, SystemTime, UNIX_EPOCH};

use lucent::engine::{HistoryEntry, NotificationId, Urgency};
use serde::{Deserialize, Serialize};
use tokio::sync::mpsc::UnboundedSender;
use tokio::sync::oneshot;
use zbus::zvariant::Type;
use zbus::{fdo, interface, proxy};

pub const CONTROL_NAME: &str = "io.github.cpt_dawn.Lucent";
pub const CONTROL_PATH: &str = "/io/github/cpt_dawn/Lucent";

/// Where the main loop sends its answer.
pub type Reply<T> = oneshot::Sender<T>;

/// Work the D-Bus thread hands to the GTK main loop, with a channel for
/// the answer.
#[derive(Debug)]
pub enum Request {
    /// Reload the config now. Answers with the config's warnings, or with
    /// why the config couldn't be used.
    ReloadConfig(Reply<Result<Vec<String>, String>>),
    /// Something the engine does or knows.
    Engine(Command),
}

/// A CLI command the engine answers.
#[derive(Debug)]
pub enum Command {
    /// Dismiss the newest Popup, or every Popup and Overflow. Answers with
    /// how many Notifications were dismissed.
    Dismiss { all: bool, reply: Reply<u32> },
    /// Restore a History entry (the most recent if `id` is `None`) as a
    /// new Popup. Answers with the entry, or `None` if there is none.
    Restore {
        id: Option<NotificationId>,
        reply: Reply<Option<HistoryItem>>,
    },
    /// Empty History. Answers with how many entries were cleared.
    ClearHistory(Reply<u32>),
    /// List History, newest first.
    History(Reply<Vec<HistoryItem>>),
}

/// A History entry as the CLI sees it, on the bus and in `--json`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
pub struct HistoryItem {
    pub id: NotificationId,
    pub app: String,
    /// Icon name or path the app sent (may be empty).
    pub icon: String,
    pub summary: String,
    /// The body as sent, possibly with markup (may be empty).
    pub body: String,
    /// `low`, `normal` or `critical`.
    pub urgency: String,
    /// When Lucent received (or last replaced) it, in Unix seconds.
    pub time: u64,
}

impl HistoryItem {
    /// Describe `entry`, dating it against the current time on both clocks.
    pub fn new(entry: &HistoryEntry, now: Instant, wall_now: SystemTime) -> Self {
        let n = &entry.notification;
        let received = wall_now - now.saturating_duration_since(entry.received_at);
        Self {
            id: n.id,
            app: n.app_name.clone(),
            icon: n.app_icon.clone(),
            summary: n.summary.clone(),
            body: n.body.clone(),
            urgency: match n.hints.urgency {
                Urgency::Low => "low",
                Urgency::Normal => "normal",
                Urgency::Critical => "critical",
            }
            .into(),
            time: received
                .duration_since(UNIX_EPOCH)
                .map_or(0, |since| since.as_secs()),
        }
    }
}

/// The object served at [`CONTROL_PATH`].
pub struct ControlServer {
    pub requests: UnboundedSender<Request>,
}

impl ControlServer {
    /// Hand a request to the main loop and await its answer.
    async fn ask<T>(&self, request: impl FnOnce(Reply<T>) -> Request) -> fdo::Result<T> {
        let shutting_down = || fdo::Error::Failed("Lucent is shutting down".into());
        let (reply, answer) = oneshot::channel();
        self.requests
            .send(request(reply))
            .map_err(|_| shutting_down())?;
        answer.await.map_err(|_| shutting_down())
    }
}

#[interface(name = "io.github.cpt_dawn.Lucent")]
impl ControlServer {
    /// Reload the config. Returns its warnings; fails with its errors if it
    /// can't be used, in which case the previous config stays.
    async fn reload(&self) -> fdo::Result<Vec<String>> {
        self.ask(Request::ReloadConfig)
            .await?
            .map_err(fdo::Error::Failed)
    }

    /// Dismiss the newest Popup, or all of them. Returns how many
    /// Notifications were dismissed.
    async fn dismiss(&self, all: bool) -> fdo::Result<u32> {
        self.ask(|reply| Request::Engine(Command::Dismiss { all, reply }))
            .await
    }

    /// Restore History entry `id` (0 means the most recent) as a new
    /// Popup. Returns `(found, entry)`; `entry` is meaningless if not found.
    async fn restore(&self, id: u32) -> fdo::Result<(bool, HistoryItem)> {
        let id = (id != 0).then_some(id);
        let restored = self
            .ask(|reply| Request::Engine(Command::Restore { id, reply }))
            .await?;
        Ok(match restored {
            Some(item) => (true, item),
            None => (false, HistoryItem::default()),
        })
    }

    /// Empty History. Returns how many entries were cleared.
    async fn clear_history(&self) -> fdo::Result<u32> {
        self.ask(|reply| Request::Engine(Command::ClearHistory(reply)))
            .await
    }

    /// History, newest first.
    async fn history(&self) -> fdo::Result<Vec<HistoryItem>> {
        self.ask(|reply| Request::Engine(Command::History(reply)))
            .await
    }
}

/// The CLI's handle on a running daemon. Calls never start the daemon.
#[proxy(
    interface = "io.github.cpt_dawn.Lucent",
    default_service = "io.github.cpt_dawn.Lucent",
    default_path = "/io/github/cpt_dawn/Lucent"
)]
pub trait Control {
    #[zbus(no_autostart)]
    fn reload(&self) -> zbus::Result<Vec<String>>;

    #[zbus(no_autostart)]
    fn dismiss(&self, all: bool) -> zbus::Result<u32>;

    #[zbus(no_autostart)]
    fn restore(&self, id: u32) -> zbus::Result<(bool, HistoryItem)>;

    #[zbus(no_autostart)]
    fn clear_history(&self) -> zbus::Result<u32>;

    #[zbus(no_autostart)]
    fn history(&self) -> zbus::Result<Vec<HistoryItem>>;
}

impl Default for HistoryItem {
    fn default() -> Self {
        Self {
            id: 0,
            app: String::new(),
            icon: String::new(),
            summary: String::new(),
            body: String::new(),
            urgency: "normal".into(),
            time: 0,
        }
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use lucent::engine::{Expiry, Hints, Notification};

    use super::*;

    #[test]
    fn a_history_item_dates_its_entry_on_the_wall_clock() {
        let now = Instant::now();
        let wall_now = UNIX_EPOCH + Duration::from_secs(1_000_000);
        let entry = HistoryEntry {
            notification: Notification {
                id: 7,
                app_name: "chat".into(),
                app_icon: "chat-icon".into(),
                summary: "hi".into(),
                body: "<b>there</b>".into(),
                actions: Vec::new(),
                hints: Hints {
                    urgency: Urgency::Critical,
                    ..Hints::default()
                },
                expiry: Expiry::Default,
            },
            received_at: now - Duration::from_secs(90),
        };

        let item = HistoryItem::new(&entry, now, wall_now);

        assert_eq!(
            item,
            HistoryItem {
                id: 7,
                app: "chat".into(),
                icon: "chat-icon".into(),
                summary: "hi".into(),
                body: "<b>there</b>".into(),
                urgency: "critical".into(),
                time: 1_000_000 - 90,
            }
        );
    }
}
