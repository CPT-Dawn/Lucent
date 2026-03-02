//! Core notification data types and inter-thread command enums.

use std::collections::HashMap;
use std::time::Instant;

/// A desktop notification received via D-Bus (freedesktop spec v1.3).
#[derive(Debug)]
#[allow(dead_code)] // Fields used in Phase 4 UI rendering.
pub struct Notification {
    /// Unique notification ID (monotonically increasing, never 0).
    pub id: u32,
    /// Name of the application that sent the notification.
    pub app_name: String,
    /// Icon name or path for the sending application.
    pub app_icon: String,
    /// Brief summary / title of the notification.
    pub summary: String,
    /// Optional body text (may contain pango markup).
    pub body: String,
    /// Action pairs: `(action_key, localized_label)`.
    pub actions: Vec<(String, String)>,
    /// Hints dictionary from the D-Bus message (`a{sv}`).
    pub hints: HashMap<String, zbus::zvariant::OwnedValue>,
    /// Expiration: -1 = server default, 0 = never, >0 = milliseconds.
    pub expire_timeout: i32,
    /// Timestamp when the notification was received.
    pub created_at: Instant,
}

/// Commands sent from the D-Bus thread → GTK UI thread.
#[derive(Debug)]
pub enum UiCommand {
    /// Display a new (or replacement) notification.
    Show(Notification),
    /// Close a notification by ID with a reason code.
    /// Reason: 1=expired, 2=dismissed by user, 3=closed by API, 4=undefined.
    Close { id: u32, reason: u32 },
    /// Recalculate popup positions after size changes.
    Reflow,
}

/// Signals sent from the UI thread → D-Bus thread for emission on the bus.
#[derive(Debug)]
#[allow(dead_code)] // Variants used in Phase 3–4.
pub enum DbusSignal {
    /// Notification was closed. Reason: 1=expired, 2=dismissed, 3=API, 4=undefined.
    Closed { id: u32, reason: u32 },
    /// User invoked an action on a notification.
    ActionInvoked { id: u32, action_key: String },
}
