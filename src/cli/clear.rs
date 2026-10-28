//! `lucent clear`: empty History.

use std::process::ExitCode;

use super::daemon::with_daemon;

pub fn run() -> ExitCode {
    with_daemon(|lucent| async move {
        match lucent.clear_history().await? {
            0 => println!("History is already empty."),
            1 => println!("Cleared 1 Notification from History."),
            n => println!("Cleared {n} Notifications from History."),
        }
        Ok(ExitCode::SUCCESS)
    })
}
