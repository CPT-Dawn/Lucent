//! UI module — notification window management, stacking, and lifecycle.
//!
//! All state lives exclusively on the GTK main thread and is driven by
//! `UiCommand` messages received from the D-Bus thread via an mpsc channel.

pub mod style;
pub mod window;

use std::collections::{HashMap, VecDeque};
use std::sync::Arc;
use std::time::Duration;

use gtk4::glib;
use tokio::sync::mpsc::UnboundedSender;

use crate::config::Config;
use crate::notification::{DbusSignal, Notification, UiCommand};
use window::NotificationWindow;

/// Vertical gap between stacked notifications.
const STACK_GAP: i32 = 10;
/// Fallback height used before a popup is fully measured.
const FALLBACK_HEIGHT: i32 = 90;

/// Shared UI state managed exclusively on the GTK main thread.
pub struct State {
    config: Arc<Config>,
    /// Channel to emit D-Bus signals back to the bus thread.
    dbus_tx: UnboundedSender<DbusSignal>,
    /// Channel to send commands to ourselves (used by auto-dismiss timers).
    ui_tx: UnboundedSender<UiCommand>,
    /// Active notification windows, keyed by notification ID.
    active: HashMap<u32, NotificationWindow>,
    /// Display order of notification IDs (top to bottom).
    display_order: Vec<u32>,
    /// Queued notifications when `max_visible_notifications` is reached.
    pending: VecDeque<Notification>,
}

impl State {
    pub fn new(
        config: Arc<Config>,
        dbus_tx: UnboundedSender<DbusSignal>,
        ui_tx: UnboundedSender<UiCommand>,
    ) -> Self {
        Self {
            config,
            dbus_tx,
            ui_tx,
            active: HashMap::new(),
            display_order: Vec::new(),
            pending: VecDeque::new(),
        }
    }

    /// Process an incoming command from the D-Bus thread or auto-dismiss timer.
    pub fn handle_command(&mut self, cmd: UiCommand) {
        match cmd {
            UiCommand::Show(notification) => self.show_notification(notification),
            UiCommand::Close { id, reason } => self.close_notification(id, reason),
            UiCommand::Reflow => self.restack(),
        }
    }

    // ── Private helpers ──────────────────────────────────────────

    /// Display a notification, or queue it if at max capacity.
    fn show_notification(&mut self, notification: Notification) {
        let id = notification.id;
        let expire_timeout = notification.expire_timeout;

        // If this replaces an existing notification, remove it silently.
        if self.active.contains_key(&id) {
            if let Some(nw) = self.active.remove(&id) {
                self.display_order.retain(|&x| x != id);
                nw.fade_out_and_destroy();
            }
        }

        // Queue if at max capacity.
        if self.active.len() >= self.config.max_visible_notifications as usize {
            eprintln!("[lucent] Queuing notification #{id} (at max visible)");
            self.pending.push_back(notification);
            return;
        }

        // Calculate vertical position based on measured heights.
        let top_offset = self.next_top_offset();

        eprintln!(
            "[lucent] #{} from \"{}\" — {}",
            id, notification.app_name, notification.summary,
        );

        // Create the layer-shell window.
        let nw = NotificationWindow::new(&notification, &self.config, top_offset, &self.ui_tx);

        self.display_order.push(id);
        self.active.insert(id, nw);

        // Reflow once immediately and once after first layout pass to handle
        // text wrapping and exact measured heights.
        self.restack();
        {
            let ui_tx = self.ui_tx.clone();
            glib::timeout_add_local_once(Duration::from_millis(30), move || {
                let _ = ui_tx.send(UiCommand::Reflow);
            });
        }

        // Schedule auto-dismiss timer.
        let timeout = if expire_timeout < 0 {
            // Use the server default.
            Duration::from_secs(self.config.timeout_seconds as u64)
        } else if expire_timeout == 0 {
            // `0` means "never expires".
            return;
        } else {
            Duration::from_millis(expire_timeout as u64)
        };

        let ui_tx = self.ui_tx.clone();
        glib::timeout_add_local_once(timeout, move || {
            let _ = ui_tx.send(UiCommand::Close { id, reason: 1 });
        });
    }

    /// Close and remove a notification, emitting the D-Bus signal.
    fn close_notification(&mut self, id: u32, reason: u32) {
        if let Some(nw) = self.active.remove(&id) {
            self.display_order.retain(|&x| x != id);

            // Tell the D-Bus thread to emit NotificationClosed.
            let _ = self.dbus_tx.send(DbusSignal::Closed { id, reason });

            // Fade out the detached window (fire-and-forget).
            nw.fade_out_and_destroy();

            eprintln!("[lucent] Closed #{id} (reason {reason})");

            // Reposition remaining windows.
            self.restack();

            // Show next queued notification if a slot opened up.
            self.show_next_pending();
        }
        // Silently ignore close for unknown IDs (e.g. duplicate timer fire).
    }

    /// Recalculate top offsets for all active windows after a removal.
    fn restack(&self) {
        let mut offset = 0;
        for id in &self.display_order {
            if let Some(nw) = self.active.get(id) {
                nw.set_top_offset(offset);
                offset += self.height_for_window(nw) + STACK_GAP;
            }
        }
    }

    fn next_top_offset(&self) -> i32 {
        let mut offset = 0;
        for id in &self.display_order {
            if let Some(nw) = self.active.get(id) {
                offset += self.height_for_window(nw) + STACK_GAP;
            }
        }
        offset
    }

    fn height_for_window(&self, window: &NotificationWindow) -> i32 {
        let measured = window.measured_height();
        if measured > 0 {
            measured
        } else {
            FALLBACK_HEIGHT
        }
    }

    /// Pop the next queued notification and show it (if a slot is free).
    fn show_next_pending(&mut self) {
        if self.active.len() < self.config.max_visible_notifications as usize {
            if let Some(notification) = self.pending.pop_front() {
                self.show_notification(notification);
            }
        }
    }
}
