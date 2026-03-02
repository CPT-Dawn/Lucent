//! Layer-shell notification window creation, positioning, and stacking.
//!
//! Each notification gets its own `gtk4::Window` configured as a
//! `gtk4_layer_shell` overlay, anchored to the top-right corner of the
//! active monitor, with automatic vertical stacking.

use std::cell::Cell;
use std::rc::Rc;
use std::time::Duration;

use gtk4::glib;
use gtk4::prelude::*;
use gtk4::{Align, Box as GtkBox, Label, Orientation, Window};
use gtk4_layer_shell::{Edge, Layer, LayerShell};

use tokio::sync::mpsc::UnboundedSender;

use crate::config::Config;
use crate::notification::{Notification, UiCommand};

/// Margin from screen edges in pixels.
const SCREEN_MARGIN: i32 = 12;

/// A live notification popup window on the Wayland layer shell.
pub struct NotificationWindow {
    pub window: Window,
    #[allow(dead_code)] // Used for debug logging; will be queried in future refinements.
    pub id: u32,
}

impl NotificationWindow {
    /// Create, configure, and present a new notification popup.
    pub fn new(
        notification: &Notification,
        config: &Config,
        top_offset: i32,
        ui_tx: &UnboundedSender<UiCommand>,
    ) -> Self {
        let window = Window::new();

        // ── Layer-shell configuration ────────────────────────────
        window.init_layer_shell();
        window.set_layer(Layer::Overlay);
        window.set_namespace(Some("lucent-notification"));

        // Anchor to top-right corner.
        window.set_anchor(Edge::Top, true);
        window.set_anchor(Edge::Right, true);
        window.set_anchor(Edge::Left, false);
        window.set_anchor(Edge::Bottom, false);

        // Position via margins.
        window.set_margin(Edge::Top, SCREEN_MARGIN + top_offset);
        window.set_margin(Edge::Right, SCREEN_MARGIN);

        // Size — natural height, fixed width.
        window.set_default_size(config.width as i32, -1);

        // ── Content layout ───────────────────────────────────────
        let container = GtkBox::new(Orientation::Vertical, 4);
        container.add_css_class("notification-popup");
        container.set_width_request(config.width as i32);

        // App name
        if !notification.app_name.is_empty() {
            let app_label = Label::new(Some(&notification.app_name));
            app_label.add_css_class("notification-app-name");
            app_label.set_halign(Align::Start);
            app_label.set_hexpand(true);
            container.append(&app_label);
        }

        // Summary
        if !notification.summary.is_empty() {
            let summary_label = Label::new(Some(&notification.summary));
            summary_label.add_css_class("notification-summary");
            summary_label.set_halign(Align::Start);
            summary_label.set_wrap(true);
            summary_label.set_max_width_chars(40);
            container.append(&summary_label);
        }

        // Body
        if !notification.body.is_empty() {
            let body_label = Label::new(Some(&notification.body));
            body_label.add_css_class("notification-body");
            body_label.set_halign(Align::Start);
            body_label.set_wrap(true);
            body_label.set_max_width_chars(40);
            body_label.set_use_markup(true);
            container.append(&body_label);
        }

        window.set_child(Some(&container));

        // ── Click-to-dismiss ─────────────────────────────────────
        let click = gtk4::GestureClick::new();
        let nid = notification.id;
        let tx = ui_tx.clone();
        click.connect_released(move |_, _, _, _| {
            let _ = tx.send(UiCommand::Close { id: nid, reason: 2 });
        });
        window.add_controller(click);

        // ── Fade-in animation ────────────────────────────────────
        window.set_opacity(0.0);
        window.present();

        let w = window.clone();
        let step = Rc::new(Cell::new(0u32));
        glib::timeout_add_local(Duration::from_millis(16), move || {
            let s = step.get() + 1;
            step.set(s);
            let progress = (s as f64 / 10.0).min(1.0); // 10 steps ≈ 160 ms
            w.set_opacity(progress);
            if s >= 10 {
                glib::ControlFlow::Break
            } else {
                glib::ControlFlow::Continue
            }
        });

        NotificationWindow {
            window,
            id: notification.id,
        }
    }

    /// Update the top margin for restacking.
    pub fn set_top_offset(&self, offset: i32) {
        self.window.set_margin(Edge::Top, SCREEN_MARGIN + offset);
    }

    /// Fade out and then close the underlying GTK window.
    ///
    /// The `NotificationWindow` is consumed — it has already been removed
    /// from the logical state so the animation is fire-and-forget.
    pub fn fade_out_and_destroy(self) {
        let window = self.window;
        let step = Rc::new(Cell::new(0u32));

        glib::timeout_add_local(Duration::from_millis(16), move || {
            let s = step.get() + 1;
            step.set(s);
            let progress = 1.0 - (s as f64 / 8.0).min(1.0); // 8 steps ≈ 128 ms
            window.set_opacity(progress);
            if s >= 8 {
                window.close();
                glib::ControlFlow::Break
            } else {
                glib::ControlFlow::Continue
            }
        });
    }
}
