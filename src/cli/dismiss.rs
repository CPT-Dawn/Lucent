//! `lucent dismiss [--all]`: close the newest Popup, or all of them.
//!
//! Quiet when it dismissed something, so it sits well behind a keybind.

use std::process::ExitCode;

use super::daemon::with_daemon;

pub fn run(all: bool) -> ExitCode {
    with_daemon(|lucent| async move {
        if lucent.dismiss(all).await? == 0 {
            println!("No Popups to dismiss.");
        }
        Ok(ExitCode::SUCCESS)
    })
}
