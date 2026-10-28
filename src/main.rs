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
    // Diagnostics go to stderr: the daemon reports its progress, while a
    // subcommand only speaks up when something is off.
    init_tracing(if cli.command.is_some() {
        "lucent=warn"
    } else {
        "lucent=info"
    });
    match cli.command {
        Some(command) => command.run(),
        None => match daemon::run() {
            Ok(()) => ExitCode::SUCCESS,
            Err(e) => {
                tracing::error!("{e}");
                ExitCode::FAILURE
            }
        },
    }
}

fn init_tracing(default_filter: &str) {
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new(default_filter)),
        )
        .with_writer(std::io::stderr)
        .init();
}
