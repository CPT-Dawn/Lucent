//! D-Bus server implementing `org.freedesktop.Notifications` (spec v1.3).
//!
//! Exposes the four required methods (GetCapabilities, Notify,
//! CloseNotification, GetServerInformation) and two signals
//! (NotificationClosed, ActionInvoked) on the session bus.

use std::collections::HashMap;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;

use tokio::sync::mpsc::UnboundedSender;
use zbus::fdo::DBusProxy;
use zbus::interface;
use zbus::names::BusName;
use zbus::object_server::SignalEmitter;
use zbus::zvariant::OwnedValue;

use crate::notification::{DbusSignal, Notification, UiCommand};

const NOTIFICATION_BUS_NAME: &str = "org.freedesktop.Notifications";

/// The notification server object exposed at
/// `/org/freedesktop/Notifications` on the session bus.
pub struct NotificationServer {
    /// Channel to push notification commands to the GTK UI thread.
    ui_tx: UnboundedSender<UiCommand>,
    /// Monotonically increasing notification ID counter (starts at 1).
    next_id: Arc<AtomicU32>,
}

impl NotificationServer {
    pub fn new(ui_tx: UnboundedSender<UiCommand>) -> Self {
        Self {
            ui_tx,
            next_id: Arc::new(AtomicU32::new(1)),
        }
    }
}

#[interface(name = "org.freedesktop.Notifications")]
impl NotificationServer {
    // ── Methods ──────────────────────────────────────────────────

    /// Returns the capabilities supported by this notification server.
    async fn get_capabilities(&self) -> Vec<String> {
        vec!["body".into(), "body-markup".into(), "icon-static".into()]
    }

    /// Sends a notification to the notification server.
    ///
    /// Returns the notification ID (> 0). If `replaces_id` is non-zero,
    /// the existing notification with that ID is atomically replaced.
    #[allow(clippy::too_many_arguments)] // Signature mandated by freedesktop spec.
    async fn notify(
        &self,
        app_name: &str,
        replaces_id: u32,
        app_icon: &str,
        summary: &str,
        body: &str,
        actions: Vec<String>,
        hints: HashMap<String, OwnedValue>,
        expire_timeout: i32,
    ) -> u32 {
        let id = if replaces_id != 0 {
            replaces_id
        } else {
            self.next_id.fetch_add(1, Ordering::Relaxed)
        };

        // Parse D-Bus action list [key, label, key, label, …] into pairs.
        let action_pairs: Vec<(String, String)> = actions
            .chunks(2)
            .filter_map(|chunk| {
                if chunk.len() == 2 {
                    Some((chunk[0].clone(), chunk[1].clone()))
                } else {
                    None
                }
            })
            .collect();

        let notification = Notification {
            id,
            app_name: app_name.to_string(),
            app_icon: app_icon.to_string(),
            summary: summary.to_string(),
            body: body.to_string(),
            actions: action_pairs,
            hints,
            expire_timeout,
            created_at: std::time::Instant::now(),
        };

        let _ = self.ui_tx.send(UiCommand::Show(notification));
        id
    }

    /// Forcefully closes and removes the notification from the user's view.
    ///
    /// Emits `NotificationClosed` with reason 3 (closed by API call).
    async fn close_notification(
        &self,
        #[zbus(signal_context)] ctxt: SignalEmitter<'_>,
        id: u32,
    ) -> zbus::fdo::Result<()> {
        let _ = self.ui_tx.send(UiCommand::Close { id, reason: 3 });

        // Emit NotificationClosed signal with reason 3 (closed by API call).
        Self::notification_closed(&ctxt, id, 3)
            .await
            .map_err(|e| zbus::fdo::Error::Failed(format!("Signal error: {e}")))?;

        Ok(())
    }

    /// Returns server identity and specification compliance version.
    async fn get_server_information(&self) -> (String, String, String, String) {
        (
            "Lucent".into(),
            "lucent".into(),
            env!("CARGO_PKG_VERSION").into(),
            "1.3".into(),
        )
    }

    // ── Signals ──────────────────────────────────────────────────

    /// Emitted when a notification is closed for any reason.
    #[zbus(signal)]
    async fn notification_closed(
        ctxt: &SignalEmitter<'_>,
        id: u32,
        reason: u32,
    ) -> zbus::Result<()>;

    /// Emitted when a user invokes an action on a notification.
    #[zbus(signal)]
    async fn action_invoked(
        ctxt: &SignalEmitter<'_>,
        id: u32,
        action_key: &str,
    ) -> zbus::Result<()>;
}

/// Build the zbus connection, claim `org.freedesktop.Notifications`,
/// serve the interface, and relay UI→D-Bus signals.
pub async fn run_server(
    ui_tx: UnboundedSender<UiCommand>,
    mut dbus_rx: tokio::sync::mpsc::UnboundedReceiver<DbusSignal>,
) -> zbus::Result<()> {
    let server = NotificationServer::new(ui_tx);

    let conn = zbus::connection::Builder::session()?
        .name(NOTIFICATION_BUS_NAME)?
        .serve_at("/org/freedesktop/Notifications", server)?
        .build()
        .await?;

    eprintln!("[lucent] D-Bus name claimed: {NOTIFICATION_BUS_NAME}");

    // Obtain signal emitter for the served interface.
    let iface_ref = conn
        .object_server()
        .interface::<_, NotificationServer>("/org/freedesktop/Notifications")
        .await?;

    // Relay signals from the UI thread onto the session bus.
    while let Some(signal) = dbus_rx.recv().await {
        let ctxt = iface_ref.signal_emitter();
        match signal {
            DbusSignal::Closed { id, reason } => {
                if let Err(e) = NotificationServer::notification_closed(ctxt, id, reason).await {
                    eprintln!("[lucent] Failed to emit NotificationClosed: {e}");
                }
            }
            DbusSignal::ActionInvoked { id, action_key } => {
                if let Err(e) = NotificationServer::action_invoked(ctxt, id, &action_key).await {
                    eprintln!("[lucent] Failed to emit ActionInvoked: {e}");
                }
            }
        }
    }

    eprintln!("[lucent] D-Bus signal channel closed");
    Ok(())
}

/// Returns the unique bus owner of `org.freedesktop.Notifications` if present.
pub async fn notifications_name_owner() -> zbus::Result<Option<String>> {
    let conn = zbus::Connection::session().await?;
    let proxy = DBusProxy::new(&conn).await?;
    let bus_name = BusName::try_from(NOTIFICATION_BUS_NAME)?;

    if proxy.name_has_owner(bus_name.clone()).await? {
        let owner = proxy.get_name_owner(bus_name).await?;
        Ok(Some(owner.to_string()))
    } else {
        Ok(None)
    }
}
