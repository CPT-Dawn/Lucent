//! `lucent status [--follow]`: the History count and Do Not Disturb state,
//! as one line of JSON that a Waybar custom module (`return-type: json`)
//! reads directly.

use std::io::Write;
use std::process::ExitCode;

use futures_util::StreamExt;
use serde_json::json;

use super::daemon::{is_not_running, with_daemon};
use crate::dbus::control::{ControlProxy, StatusItem};

pub fn run(follow: bool) -> ExitCode {
    with_daemon(|lucent| async move {
        if follow {
            self::follow(lucent).await
        } else {
            let status = lucent.status().await?;
            Ok(print(status).unwrap_or(ExitCode::SUCCESS))
        }
    })
}

/// Print the status now and a new line on every `StatusChanged` signal,
/// until stdout closes. Nothing polls: the process sleeps between signals.
///
/// A daemon that isn't running has an empty History and Do Not Disturb off
/// (both reset on launch), so that is what is shown until it starts, and
/// again if it stops.
async fn follow(lucent: ControlProxy<'static>) -> zbus::Result<ExitCode> {
    // Subscribe before asking, so no change can slip in between.
    let mut changes = lucent.receive_status_changed().await?;
    let mut owners = lucent.inner().receive_owner_changed().await?;
    let status = match lucent.status().await {
        Ok(status) => status,
        Err(e) if is_not_running(&e) => StatusItem::default(),
        Err(e) => return Err(e),
    };
    if let Some(done) = print(status) {
        return Ok(done);
    }
    let mut shown = status;
    loop {
        let status = tokio::select! {
            Some(signal) = changes.next() => match signal.args() {
                Ok(args) => args.status,
                Err(e) => {
                    eprintln!("lucent: ignoring a malformed status signal: {e}");
                    continue;
                }
            },
            Some(owner) = owners.next() => match owner {
                // A freshly started daemon: ask rather than assume.
                Some(_) => match lucent.status().await {
                    Ok(status) => status,
                    Err(e) if is_not_running(&e) => StatusItem::default(),
                    Err(e) => return Err(e),
                },
                None => StatusItem::default(),
            },
            else => return Ok(ExitCode::SUCCESS),
        };
        if status == shown {
            continue;
        }
        if let Some(done) = print(status) {
            return Ok(done);
        }
        shown = status;
    }
}

/// Write one status line and flush it. Returns an exit code if stdout is
/// gone (the bar went away), which ends `--follow` quietly.
fn print(status: StatusItem) -> Option<ExitCode> {
    let mut out = std::io::stdout().lock();
    writeln!(out, "{}", line(status))
        .and_then(|()| out.flush())
        .err()
        .map(|_| ExitCode::SUCCESS)
}

/// One status line: Waybar's `text`, `alt`, `tooltip` and `class`, plus the
/// raw `count` and `dnd` for other scripts.
///
/// `alt` (and `class`) is `none`, `notification`, `dnd-none` or
/// `dnd-notification`, for Waybar's `format-icons` and CSS.
fn line(status: StatusItem) -> String {
    let StatusItem { history_count, dnd } = status;
    let kind = match (dnd, history_count) {
        (false, 0) => "none",
        (false, _) => "notification",
        (true, 0) => "dnd-none",
        (true, _) => "dnd-notification",
    };
    let mut tooltip = match history_count {
        0 => "No Notifications in History".to_owned(),
        1 => "1 Notification in History".to_owned(),
        n => format!("{n} Notifications in History"),
    };
    if dnd {
        tooltip.push_str("\nDo Not Disturb is on");
    }
    json!({
        "text": history_count.to_string(),
        "alt": kind,
        "tooltip": tooltip,
        "class": kind,
        "count": history_count,
        "dnd": dnd,
    })
    .to_string()
}

#[cfg(test)]
mod tests {
    use serde_json::{Value, json};

    use super::*;

    fn status(history_count: u32, dnd: bool) -> Value {
        let text = line(StatusItem { history_count, dnd });
        assert!(!text.contains('\n'), "Waybar needs one line: {text:?}");
        serde_json::from_str(&text).unwrap()
    }

    #[test]
    fn an_empty_history_with_do_not_disturb_off() {
        assert_eq!(
            status(0, false),
            json!({
                "text": "0",
                "alt": "none",
                "tooltip": "No Notifications in History",
                "class": "none",
                "count": 0,
                "dnd": false,
            })
        );
    }

    #[test]
    fn notifications_in_history() {
        assert_eq!(
            status(3, false),
            json!({
                "text": "3",
                "alt": "notification",
                "tooltip": "3 Notifications in History",
                "class": "notification",
                "count": 3,
                "dnd": false,
            })
        );
        assert_eq!(status(1, false)["tooltip"], "1 Notification in History");
    }

    #[test]
    fn do_not_disturb_shows_in_alt_class_and_tooltip() {
        assert_eq!(
            status(2, true),
            json!({
                "text": "2",
                "alt": "dnd-notification",
                "tooltip": "2 Notifications in History\nDo Not Disturb is on",
                "class": "dnd-notification",
                "count": 2,
                "dnd": true,
            })
        );
        assert_eq!(status(0, true)["alt"], "dnd-none");
    }
}
