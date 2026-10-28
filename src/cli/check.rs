//! `lucent check`: validate the config without touching the daemon.

use std::path::{Path, PathBuf};
use std::process::ExitCode;

use lucent::config::{Diagnostic, file};

pub fn run(path: Option<PathBuf>) -> ExitCode {
    let Some(path) = path.or_else(file::path) else {
        eprintln!("lucent: can't find your config directory (is $HOME set?)");
        return ExitCode::FAILURE;
    };
    if !path.exists() {
        println!(
            "{} doesn't exist, so Lucent uses its defaults. \
             Lucent writes a commented one when it starts.",
            path.display()
        );
        return ExitCode::SUCCESS;
    }

    match file::load(&path) {
        Ok(parsed) => {
            print_all(&path, "warning", &parsed.warnings);
            println!("{} is valid{}.", path.display(), counted(&parsed.warnings));
            ExitCode::SUCCESS
        }
        Err(file::LoadError::Invalid(rejected)) => {
            let mut all: Vec<_> = rejected
                .errors
                .iter()
                .map(|d| ("error", d))
                .chain(rejected.warnings.iter().map(|d| ("warning", d)))
                .collect();
            all.sort_by_key(|(_, d)| (d.line, d.column));
            for (severity, diagnostic) in all {
                print(&path, severity, diagnostic);
            }
            let n = rejected.errors.len();
            println!(
                "{} has {n} error{}; Lucent keeps its previous config until they're fixed.",
                path.display(),
                if n == 1 { "" } else { "s" }
            );
            ExitCode::FAILURE
        }
        Err(e @ file::LoadError::Unreadable(_)) => {
            eprintln!("lucent: {}: {e}", path.display());
            ExitCode::FAILURE
        }
    }
}

fn print_all(path: &Path, severity: &str, diagnostics: &[Diagnostic]) {
    for diagnostic in diagnostics {
        print(path, severity, diagnostic);
    }
}

/// `path:line:column: severity: message`, then the fix.
fn print(path: &Path, severity: &str, d: &Diagnostic) {
    println!(
        "{}:{}:{}: {severity}: {}",
        path.display(),
        d.line,
        d.column,
        d.message
    );
    if let Some(help) = &d.help {
        println!("  help: {help}");
    }
}

fn counted(warnings: &[Diagnostic]) -> String {
    match warnings.len() {
        0 => String::new(),
        1 => " (1 warning)".into(),
        n => format!(" ({n} warnings)"),
    }
}
