//! D-Bus adapter: serves `org.freedesktop.Notifications` (spec 1.3) on its
//! own thread, turns calls into engine [`Event`]s and emits the signals the
//! engine asks for.

pub mod control;
pub mod hints;
pub mod owner;

use std::collections::HashMap;

use lucent::engine::{Action, Effect, Event, Expiry, IdAllocator, Notification};
use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender};
use zbus::OwnedVariant;
use zbus::fdo::DBusProxy;
use zbus::interface;
use zbus::names::BusName;
use zbus::object_server::SignalEmitter;

const NOTIFICATIONS_NAME: &str = "org.freedesktop.Notifications";
const NOTIFICATIONS_PATH: &str = "/org/freedesktop/Notifications";

/// The object served at `/org/freedesktop/Notifications`.
struct NotificationServer {
    events: UnboundedSender<Event>,
    ids: IdAllocator,
}

#[interface(name = "org.freedesktop.Notifications")]
impl NotificationServer {
    /// Only what Popups actually render; later tickets add the rest as
    /// they land.
    async fn get_capabilities(&self) -> Vec<String> {
        [
            "body",
            "body-markup",
            "body-hyperlinks",
            "icon-static",
            "actions",
            "persistence",
        ]
        .map(String::from)
        .to_vec()
    }

    #[allow(clippy::too_many_arguments)] // Signature mandated by the spec.
    async fn notify(
        &self,
        app_name: String,
        replaces_id: u32,
        app_icon: String,
        summary: String,
        body: String,
        actions: Vec<String>,
        hints: HashMap<String, OwnedVariant>,
        expire_timeout: i32,
    ) -> u32 {
        let id = if replaces_id != 0 {
            replaces_id
        } else {
            self.ids.next()
        };
        let notification = Notification {
            id,
            app_name,
            app_icon,
            summary,
            body,
            actions: actions
                .as_chunks::<2>()
                .0
                .iter()
                .map(|[key, label]| Action {
                    key: key.clone(),
                    label: label.clone(),
                })
                .collect(),
            hints: hints::parse(&hints),
            expiry: Expiry::from_millis(expire_timeout),
        };
        tracing::debug!(id, app = %notification.app_name, "Notification received");
        self.send(Event::NotificationReceived(notification));
        id
    }

    /// The engine emits `NotificationClosed` (reason 3) if the id is known.
    async fn close_notification(&self, id: u32) {
        self.send(Event::AppClosed(id));
    }

    async fn get_server_information(&self) -> (String, String, String, String) {
        (
            "Lucent".into(),
            "lucent".into(),
            env!("CARGO_PKG_VERSION").into(),
            "1.3".into(),
        )
    }

    #[zbus(signal)]
    async fn notification_closed(
        emitter: &SignalEmitter<'_>,
        id: u32,
        reason: u32,
    ) -> zbus::Result<()>;

    #[zbus(signal)]
    async fn action_invoked(
        emitter: &SignalEmitter<'_>,
        id: u32,
        action_key: &str,
    ) -> zbus::Result<()>;
}

impl NotificationServer {
    fn send(&self, event: Event) {
        if self.events.send(event).is_err() {
            tracing::warn!("engine is gone; dropping event");
        }
    }
}

/// Whether this effect is for the D-Bus thread (signals and status).
pub fn handles(effect: &Effect) -> bool {
    matches!(
        effect,
        Effect::NotificationClosed { .. } | Effect::ActionInvoked { .. } | Effect::StatusChanged(_)
    )
}

/// Claim the notification and control names, serve both interfaces and emit the signal
/// effects that arrive on `effects` until the channel closes.
pub async fn run_server(
    events: UnboundedSender<Event>,
    ids: IdAllocator,
    requests: UnboundedSender<control::Request>,
    mut effects: UnboundedReceiver<Effect>,
) -> zbus::Result<()> {
    let conn = zbus::connection::Builder::session()
        .serve_at(NOTIFICATIONS_PATH, NotificationServer { events, ids })
        .serve_at(control::CONTROL_PATH, control::ControlServer { requests })
        .name(NOTIFICATIONS_NAME)
        .name(control::CONTROL_NAME)
        // Never steal the name from another daemon, nor let one steal ours.
        .replace_existing_names(false)
        .allow_name_replacements(false)
        .build()
        .await?;
    tracing::info!("claimed {NOTIFICATIONS_NAME} and {}", control::CONTROL_NAME);

    let iface = conn
        .object_server()
        .interface::<_, NotificationServer>(NOTIFICATIONS_PATH)
        .await?;
    let control = conn
        .object_server()
        .interface::<_, control::ControlServer>(control::CONTROL_PATH)
        .await?;

    while let Some(effect) = effects.recv().await {
        let emitter = iface.signal_emitter();
        let result = match effect {
            Effect::NotificationClosed { id, reason } => {
                NotificationServer::notification_closed(emitter, id, reason.code()).await
            }
            Effect::ActionInvoked { id, key } => {
                NotificationServer::action_invoked(emitter, id, &key).await
            }
            Effect::StatusChanged(status) => {
                control::ControlServer::status_changed(control.signal_emitter(), status.into())
                    .await
            }
            _ => Ok(()),
        };
        if let Err(e) = result {
            tracing::warn!("failed to emit signal: {e}");
        }
    }
    Ok(())
}

/// Whoever currently owns `org.freedesktop.Notifications`, as far as it
/// can be identified, or `None` if nobody does.
pub async fn notifications_owner(conn: &zbus::Connection) -> zbus::Result<Option<owner::Owner>> {
    let dbus = DBusProxy::new(conn).await?;
    let name = BusName::try_from(NOTIFICATIONS_NAME)?;
    if !dbus.name_has_owner(name.clone()).await? {
        return Ok(None);
    }
    let unique = BusName::from(dbus.get_name_owner(name).await?.into_inner());
    let pid = dbus
        .get_connection_unix_process_id(unique.clone())
        .await
        .ok();
    let proc_file = |file: &str| {
        pid.and_then(|pid| std::fs::read_to_string(format!("/proc/{pid}/{file}")).ok())
    };
    Ok(Some(owner::Owner {
        server_name: server_name(conn, unique).await,
        process: proc_file("comm")
            .map(|comm| comm.trim().to_owned())
            .filter(|comm| !comm.is_empty()),
        pid,
        unit: proc_file("cgroup").and_then(|cgroup| owner::systemd_unit(&cgroup)),
    }))
}

/// The name a notification server gives in `GetServerInformation`. A
/// daemon that doesn't answer promptly stays anonymous.
async fn server_name(conn: &zbus::Connection, server: BusName<'_>) -> Option<String> {
    let proxy = zbus::Proxy::new(conn, server, NOTIFICATIONS_PATH, NOTIFICATIONS_NAME)
        .await
        .ok()?;
    let call = proxy.call::<_, _, (String, String, String, String)>("GetServerInformation", &());
    let (name, ..) = tokio::time::timeout(std::time::Duration::from_millis(500), call)
        .await
        .ok()?
        .ok()?;
    Some(name).filter(|name| !name.is_empty())
}
