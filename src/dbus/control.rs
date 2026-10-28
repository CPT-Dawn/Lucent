//! Lucent's private control interface, `io.github.cpt_dawn.Lucent`, used
//! by the `lucent` subcommands.
//!
//! The daemon serves [`ControlServer`]; the CLI talks to it through
//! [`ControlProxy`]. To add a command, add a method to both. Commands that
//! change engine state send an engine `Event`; commands the GTK main loop
//! must handle itself (like reloading the config) send a [`Request`].

use tokio::sync::mpsc::UnboundedSender;
use tokio::sync::oneshot;
use zbus::{fdo, interface, proxy};

pub const CONTROL_NAME: &str = "io.github.cpt_dawn.Lucent";
pub const CONTROL_PATH: &str = "/io/github/cpt_dawn/Lucent";

/// Work the D-Bus thread hands to the GTK main loop, with a channel for
/// the answer.
#[derive(Debug)]
pub enum Request {
    /// Reload the config now. Answers with the config's warnings, or with
    /// why the config couldn't be used.
    ReloadConfig(oneshot::Sender<Result<Vec<String>, String>>),
}

/// The object served at [`CONTROL_PATH`].
pub struct ControlServer {
    pub requests: UnboundedSender<Request>,
}

#[interface(name = "io.github.cpt_dawn.Lucent")]
impl ControlServer {
    /// Reload the config. Returns its warnings; fails with its errors if it
    /// can't be used, in which case the previous config stays.
    async fn reload(&self) -> fdo::Result<Vec<String>> {
        let (reply, answer) = oneshot::channel();
        self.requests
            .send(Request::ReloadConfig(reply))
            .map_err(|_| fdo::Error::Failed("Lucent is shutting down".into()))?;
        answer
            .await
            .map_err(|_| fdo::Error::Failed("Lucent is shutting down".into()))?
            .map_err(fdo::Error::Failed)
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
}
