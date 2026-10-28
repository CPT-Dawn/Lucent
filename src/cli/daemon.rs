//! Reaching the running daemon from a subcommand, with a clear message
//! when it isn't running.

use std::future::Future;
use std::process::ExitCode;

use crate::dbus::control::ControlProxy;

/// Run `command` against the daemon's control interface on a small tokio
/// runtime. Connection failures and "not running" are reported here, so
/// `command` only handles its own replies.
pub fn with_daemon<F, Fut>(command: F) -> ExitCode
where
    F: FnOnce(ControlProxy<'static>) -> Fut,
    Fut: Future<Output = Result<ExitCode, zbus::Error>>,
{
    let runtime = match tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
    {
        Ok(runtime) => runtime,
        Err(e) => {
            eprintln!("lucent: can't start: {e}");
            return ExitCode::FAILURE;
        }
    };
    let result = runtime.block_on(async {
        let conn = zbus::Connection::session().await?;
        let proxy = ControlProxy::new(&conn).await?;
        command(proxy).await
    });
    match result {
        Ok(code) => code,
        Err(e) if is_not_running(&e) => {
            eprintln!(
                "lucent: Lucent isn't running. It starts by itself when an app sends a \
                 Notification, or run `lucent` to start it now."
            );
            ExitCode::FAILURE
        }
        Err(e) => {
            eprintln!("lucent: can't reach the daemon: {e}");
            ExitCode::FAILURE
        }
    }
}

fn is_not_running(error: &zbus::Error) -> bool {
    matches!(
        error,
        zbus::Error::FDO(e) if matches!(
            **e,
            zbus::fdo::Error::ServiceUnknown(_) | zbus::fdo::Error::NameHasNoOwner(_)
        )
    )
}
