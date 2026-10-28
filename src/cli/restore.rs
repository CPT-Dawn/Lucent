//! `lucent restore [id]`: show a History entry again as a new Popup.

use std::process::ExitCode;

use super::daemon::with_daemon;

pub fn run(id: Option<u32>) -> ExitCode {
    with_daemon(|lucent| async move {
        let (found, item) = lucent.restore(id.unwrap_or(0)).await?;
        if found {
            println!("Restored \"{}\".", item.summary);
            return Ok(ExitCode::SUCCESS);
        }
        match id {
            None => eprintln!("lucent: History is empty, so there's nothing to restore."),
            Some(id) => {
                eprintln!("lucent: History has no entry {id}. Run `lucent history` to see the ids.")
            }
        }
        Ok(ExitCode::FAILURE)
    })
}
