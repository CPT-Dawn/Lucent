//! `lucent dnd [on|off|toggle]`: switch Do Not Disturb, or show its state.

use std::process::ExitCode;

use super::DndChange;
use super::daemon::with_daemon;

pub fn run(change: Option<DndChange>) -> ExitCode {
    with_daemon(|lucent| async move {
        let status = match change {
            None => lucent.status().await?,
            Some(change) => {
                let change = match change {
                    DndChange::On => "on",
                    DndChange::Off => "off",
                    DndChange::Toggle => "toggle",
                };
                lucent.set_dnd(change).await?
            }
        };
        if status.dnd {
            println!("Do Not Disturb is on.");
        } else {
            println!("Do Not Disturb is off.");
        }
        Ok(ExitCode::SUCCESS)
    })
}
