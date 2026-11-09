//! Stderr logging without a framework: errors always, debug output only with
//! `LUCENT_LOG=debug`. There is no level in between: a config problem the
//! user can fix (an unknown key, Preset or monitor) is an error, and
//! anything else worth saying is debug output.

use std::sync::atomic::{AtomicBool, Ordering};

static VERBOSE: AtomicBool = AtomicBool::new(false);

/// Reads `LUCENT_LOG` once at startup.
pub fn init() {
    let verbose = std::env::var("LUCENT_LOG").is_ok_and(|v| v.eq_ignore_ascii_case("debug"));
    VERBOSE.store(verbose, Ordering::Relaxed);
}

pub fn verbose() -> bool {
    VERBOSE.load(Ordering::Relaxed)
}

/// Writes one log line to stderr. A broken stderr (a closed pipe, a hung-up
/// terminal) loses the line rather than the daemon: `eprintln!` would panic.
pub fn write(level: &str, message: std::fmt::Arguments) {
    use std::io::Write;
    let _ = writeln!(std::io::stderr(), "lucent: {level}: {message}");
}

#[macro_export]
macro_rules! error {
    ($($arg:tt)*) => {
        $crate::log::write("error", format_args!($($arg)*))
    };
}

#[macro_export]
macro_rules! debug {
    ($($arg:tt)*) => {
        if $crate::log::verbose() {
            $crate::log::write("debug", format_args!($($arg)*))
        }
    };
}
