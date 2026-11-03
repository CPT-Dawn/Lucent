use std::process::ExitCode;

use lucent::app::{self, Exit};
use lucent::{config, error, osd};

fn main() -> ExitCode {
    lucent::log::init();
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.first().map(String::as_str) {
        None => {}
        Some("--default-config") => {
            print!("{}", config::REFERENCE);
            return ExitCode::SUCCESS;
        }
        Some("osd") => {
            return match osd::run(&args[1..]) {
                Ok(()) => ExitCode::SUCCESS,
                Err(e) => {
                    error!("{e}");
                    ExitCode::FAILURE
                }
            };
        }
        // A typo must not start a daemon nobody asked for.
        Some(arg) => {
            error!("unknown argument `{arg}`; try --default-config or osd");
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
