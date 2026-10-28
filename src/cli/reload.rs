//! `lucent reload`: make the running daemon reload its config now.

use std::process::ExitCode;

use super::daemon::with_daemon;

pub fn run() -> ExitCode {
    with_daemon(|lucent| async move {
        match lucent.reload().await {
            Ok(warnings) => {
                for warning in &warnings {
                    println!("warning: {warning}");
                }
                println!("Reloaded the config.");
                Ok(ExitCode::SUCCESS)
            }
            Err(zbus::Error::FDO(e)) => match *e {
                zbus::fdo::Error::Failed(why) => {
                    eprintln!("lucent: {why}");
                    eprintln!("Lucent kept its previous config. Run `lucent check` for details.");
                    Ok(ExitCode::FAILURE)
                }
                other => Err(zbus::Error::FDO(Box::new(other))),
            },
            Err(e) => Err(e),
        }
    })
}
