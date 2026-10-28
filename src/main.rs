//! Lucent, a Wayland notification daemon, and the `lucent` command that
//! controls it. See [`cli`] for the subcommands and [`daemon`] for the
//! daemon itself.

mod cli;
mod daemon;
mod dbus;
mod engine_host;
mod live_config;
mod timers;
mod ui;

use std::process::ExitCode;

use clap::Parser;
use tracing_subscriber::EnvFilter;

fn main() -> ExitCode {
    let cli = cli::Cli::parse();
    match cli.command {
        Some(command) => command.run(),
        None => {
            tracing_subscriber::fmt()
                .with_env_filter(
                    EnvFilter::try_from_default_env()
                        .unwrap_or_else(|_| EnvFilter::new("lucent=info")),
                )
                .with_writer(std::io::stderr)
                .init();
            match daemon::run() {
                Ok(()) => ExitCode::SUCCESS,
                Err(e) => {
                    tracing::error!("{e}");
                    ExitCode::FAILURE
                }
            }
        }
    }
}
