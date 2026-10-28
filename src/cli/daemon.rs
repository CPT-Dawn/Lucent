//! Reaching the running daemon from a subcommand, with a clear next step
//! when it can't be reached.

use std::future::Future;
use std::process::ExitCode;

use crate::dbus::control::ControlProxy;
use crate::dbus::notifications_owner;
use crate::dbus::owner::{Owner, instead_of_lucent};

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
    let failure = runtime.block_on(async {
        let conn = match zbus::Connection::session().await {
            Ok(conn) => conn,
            Err(e) => return Err(Unreachable::NoSessionBus(e)),
        };
        let result = match ControlProxy::new(&conn).await {
            Ok(proxy) => command(proxy).await,
            Err(e) => Err(e),
        };
        match result {
            Ok(code) => Ok(code),
            Err(e) if is_not_running(&e) => Err(Unreachable::NotRunning(
                notifications_owner(&conn).await.ok().flatten(),
            )),
            Err(e) => Err(Unreachable::Failed(e)),
        }
    });
    match failure {
        Ok(code) => code,
        Err(unreachable) => {
            eprintln!("lucent: {}", unreachable.message().trim_end());
            ExitCode::FAILURE
        }
    }
}

/// What to do about Lucent not running, for a command that carries on
/// without it (`lucent status --follow`).
pub async fn not_running_message(conn: &zbus::Connection) -> String {
    Unreachable::NotRunning(notifications_owner(conn).await.ok().flatten()).message()
}

/// Why a subcommand couldn't talk to Lucent.
#[derive(Debug)]
enum Unreachable {
    /// There is no D-Bus session bus to connect to.
    NoSessionBus(zbus::Error),
    /// Lucent isn't running; maybe another daemon owns the notification
    /// name instead.
    NotRunning(Option<Owner>),
    /// Lucent is running but the call failed.
    Failed(zbus::Error),
}

impl Unreachable {
    /// What went wrong, then what to do about it.
    fn message(&self) -> String {
        match self {
            Self::NoSessionBus(e) => format!(
                "can't connect to the D-Bus session bus ({e}).\n\
                 Lucent lives on the session bus: run this from inside your desktop session, \
                 where DBUS_SESSION_BUS_ADDRESS is set."
            ),
            Self::NotRunning(Some(owner)) if !owner.is_lucent() => {
                format!("Lucent isn't running: {}", instead_of_lucent(owner))
            }
            Self::NotRunning(_) => "Lucent isn't running. It starts by itself when an app sends a \
                 Notification, or run `lucent` to start it now."
                .into(),
            Self::Failed(e) => format!(
                "Lucent didn't answer: {e}\n\
                 If it keeps happening, restart it: `pkill -x lucent && lucent`."
            ),
        }
    }
}

/// Whether `error` means the daemon isn't running.
pub fn is_not_running(error: &zbus::Error) -> bool {
    matches!(
        error,
        zbus::Error::FDO(e) if matches!(
            **e,
            zbus::fdo::Error::ServiceUnknown(_) | zbus::fdo::Error::NameHasNoOwner(_)
        )
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn with_nobody_in_its_place_the_next_step_is_to_start_lucent() {
        let text = Unreachable::NotRunning(None).message();
        assert!(text.starts_with("Lucent isn't running."), "{text}");
        assert!(text.contains("run `lucent` to start it"), "{text}");
    }

    #[test]
    fn with_another_daemon_in_its_place_the_next_step_is_to_switch() {
        let mako = Owner {
            process: Some("mako".into()),
            ..Owner::default()
        };
        let text = Unreachable::NotRunning(Some(mako)).message();
        assert!(
            text.starts_with("Lucent isn't running: mako is handling Notifications instead."),
            "{text}"
        );
        assert!(text.contains("pkill -x mako && lucent"), "{text}");
    }

    #[test]
    fn without_a_session_bus_it_says_where_to_run_it() {
        let text = Unreachable::NoSessionBus(zbus::Error::Unsupported).message();
        assert!(text.contains("DBUS_SESSION_BUS_ADDRESS"), "{text}");
    }
}
