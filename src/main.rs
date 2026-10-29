use std::process::ExitCode;

use lucent::app::{self, Exit};
use lucent::{config, error};

fn main() -> ExitCode {
    lucent::log::init();
    match std::env::args().nth(1).as_deref() {
        None => {}
        Some("--default-config") => {
            print!("{}", config::REFERENCE);
            return ExitCode::SUCCESS;
        }
        // A typo must not start a daemon nobody asked for.
        Some(arg) => {
            error!("unknown argument `{arg}`; try --default-config");
            return ExitCode::FAILURE;
        }
    }
    match app::run(config::load()) {
        // Another daemon holding the name is not Lucent's failure.
        Ok(Exit::Done | Exit::NameTaken) => ExitCode::SUCCESS,
        Err(e) => {
            error!("{e}");
            ExitCode::FAILURE
        }
    }
}
