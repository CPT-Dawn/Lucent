//! `lucent demo`: plays [`lucent::demo::script`] to the running daemon
//! over the ordinary `org.freedesktop.Notifications` interface, exactly as
//! an app would.

use std::collections::HashMap;
use std::process::ExitCode;

use lucent::demo::script;
use lucent::engine::{Image, Notification, NotificationId, Urgency};
use zbus::{OwnedVariant, Variant as Value};

use super::daemon::with_daemon;

pub fn run() -> ExitCode {
    with_daemon(|lucent| async move {
        // Asking first means Lucent (not some other daemon) is the one
        // that will get these, and gives the friendly "not running" hint.
        lucent.status().await?;
        let notifications = zbus::Proxy::new(
            lucent.inner().connection(),
            "org.freedesktop.Notifications",
            "/org/freedesktop/Notifications",
            "org.freedesktop.Notifications",
        )
        .await?;

        println!("Lights down, Popups up: every kind of Popup in about seven seconds.");
        let mut ids: HashMap<NotificationId, u32> = HashMap::new();
        for step in script() {
            tokio::time::sleep(step.wait).await;
            let n = &step.notification;
            let replaces = ids.get(&n.id).copied().unwrap_or(0);
            let id: u32 = notifications
                .call("Notify", &notify_args(n, replaces))
                .await?;
            ids.insert(n.id, id);
        }
        println!(
            "That's the show. Hover a Popup to pause it, click an Action, right-click to \
             dismiss,\nmiddle-click to dismiss them all. The Critical one stays until you \
             do (or run `lucent dismiss`).\nThey're all in `lucent history` now."
        );
        Ok(ExitCode::SUCCESS)
    })
}

/// The arguments of `Notify` that carry `n`, as an app would send them.
type NotifyArgs<'a> = (
    &'a str,
    u32,
    &'a str,
    &'a str,
    &'a str,
    Vec<&'a str>,
    HashMap<&'static str, OwnedVariant>,
    i32,
);

fn notify_args(n: &Notification, replaces: u32) -> NotifyArgs<'_> {
    let actions = n
        .actions
        .iter()
        .flat_map(|a| [a.key.as_str(), a.label.as_str()])
        .collect();
    let expire_timeout = match n.expiry {
        lucent::engine::Expiry::Default => -1,
        lucent::engine::Expiry::Never => 0,
        lucent::engine::Expiry::After(after) => {
            i32::try_from(after.as_millis()).unwrap_or(i32::MAX)
        }
    };
    (
        &n.app_name,
        replaces,
        &n.app_icon,
        &n.summary,
        &n.body,
        actions,
        hints(n),
        expire_timeout,
    )
}

fn hints(n: &Notification) -> HashMap<&'static str, OwnedVariant> {
    let mut hints = HashMap::new();
    let mut put = |key, value: Value<'_>| {
        if let Ok(value) = OwnedVariant::try_from(value) {
            hints.insert(key, value);
        }
    };
    let urgency: u8 = match n.hints.urgency {
        Urgency::Low => 0,
        Urgency::Normal => 1,
        Urgency::Critical => 2,
    };
    put("urgency", Value::from(urgency));
    if let Some(percent) = n.hints.progress {
        put("value", Value::from(i32::from(percent)));
    }
    match &n.hints.image {
        Some(Image::Data(art)) => put(
            "image-data",
            Value::from((
                art.width,
                art.height,
                art.rowstride,
                art.has_alpha,
                art.bits_per_sample,
                art.channels,
                art.data.to_vec(),
            )),
        ),
        Some(Image::Path(path)) => put(
            "image-path",
            Value::from(path.to_string_lossy().into_owned()),
        ),
        None => {}
    }
    if n.hints.transient {
        put("transient", Value::from(true));
    }
    hints
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dbus::hints;
    use crate::ui::icon::{Art, plan};

    #[test]
    fn every_demo_notification_arrives_as_it_was_written() {
        for step in script() {
            let n = step.notification;
            let (app, _, icon, summary, body, actions, raw, _) = notify_args(&n, 0);
            let raw = raw.into_iter().map(|(k, v)| (k.to_owned(), v)).collect();
            assert_eq!(hints::parse(&raw), n.hints, "{summary}");
            assert_eq!((app, icon, body), (&*n.app_name, &*n.app_icon, &*n.body));
            assert_eq!(actions.len(), n.actions.len() * 2);
        }
    }

    #[test]
    fn the_album_art_fills_its_tile() {
        let art = script()
            .into_iter()
            .find(|s| s.notification.hints.image.is_some())
            .unwrap()
            .notification;
        assert!(matches!(plan(&art).art, Some(Art::Pixels(_))));
    }

    #[test]
    fn expiry_is_sent_in_milliseconds() {
        let expiries: Vec<i32> = script()
            .iter()
            .map(|s| notify_args(&s.notification, 0).7)
            .collect();
        assert!(expiries.contains(&-1));
        assert!(expiries.contains(&14_000));
    }
}
