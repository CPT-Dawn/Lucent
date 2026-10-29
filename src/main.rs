use std::process::ExitCode;

use lucent::app::{self, Exit};
use lucent::error;

fn main() -> ExitCode {
    lucent::log::init();
    match app::run() {
        // Another daemon holding the name is not Lucent's failure.
        Ok(Exit::Done | Exit::NameTaken) => ExitCode::SUCCESS,
        Err(e) => {
            error!("{e}");
            ExitCode::FAILURE
        }
    }
}
