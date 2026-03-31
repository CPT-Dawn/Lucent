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
/// Vertical travel distance for enter/exit animation in pixels.
const ENTRY_OFFSET_PX: f64 = 14.0;
/// Enter animation duration.
const ENTER_DURATION: Duration = Duration::from_millis(220);
/// Exit animation duration.
const EXIT_DURATION: Duration = Duration::from_millis(160);
/// Re-stack animation duration.
const RESTACK_DURATION: Duration = Duration::from_millis(180);

fn ease_out_cubic(t: f64) -> f64 {
    1.0 - (1.0 - t).powi(3)
}

fn ease_in_cubic(t: f64) -> f64 {
    t.powi(3)
}

/// A live notification popup window on the Wayland layer shell.
pub struct NotificationWindow {
    pub window: Window,
    #[allow(dead_code)] // Used for debug logging; will be queried in future refinements.
    pub id: u32,
    current_top_offset: Rc<Cell<f64>>,
    animation_epoch: Rc<Cell<u64>>,
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
        window.set_decorated(false);
        window.set_resizable(false);
        window.add_css_class("lucent-window");

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
        window.set_margin(
            Edge::Top,
            SCREEN_MARGIN + top_offset + ENTRY_OFFSET_PX as i32,
        );
        window.set_margin(Edge::Right, SCREEN_MARGIN);

        // Size — natural height, fixed width.
        window.set_default_size(config.width as i32, -1);

        // ── Content layout ───────────────────────────────────────
        let container = GtkBox::new(Orientation::Vertical, 6);
        container.add_css_class("notification-popup");
        container.set_width_request(config.width as i32);

        // App name
        if !notification.app_name.is_empty() {
            let app_label = Label::new(Some(&notification.app_name));
            app_label.add_css_class("notification-app-name");
            app_label.set_halign(Align::Start);
            app_label.set_xalign(0.0);
            app_label.set_hexpand(true);
            container.append(&app_label);
        }

        // Summary
        if !notification.summary.is_empty() {
            let summary_label = Label::new(Some(&notification.summary));
            summary_label.add_css_class("notification-summary");
            summary_label.set_halign(Align::Start);
            summary_label.set_xalign(0.0);
            summary_label.set_wrap(true);
            summary_label.set_max_width_chars(40);
            container.append(&summary_label);
        }

        // Body
        if !notification.body.is_empty() {
            let body_label = Label::new(Some(&notification.body));
            body_label.add_css_class("notification-body");
            body_label.set_halign(Align::Start);
            body_label.set_xalign(0.0);
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

        let current_top_offset = Rc::new(Cell::new(top_offset as f64 + ENTRY_OFFSET_PX));
        let animation_epoch = Rc::new(Cell::new(0u64));

        {
            let window = window.clone();
            let animated_window = window.clone();
            let current_top_offset = current_top_offset.clone();
            let animation_epoch = animation_epoch.clone();
            let start_time = Rc::new(Cell::new(None::<i64>));
            let epoch = animation_epoch.get().wrapping_add(1);
            animation_epoch.set(epoch);

            window.add_tick_callback(move |_widget, frame_clock| {
                if animation_epoch.get() != epoch {
                    return glib::ControlFlow::Break;
                }

                let frame_time = frame_clock.frame_time();
                let start = match start_time.get() {
                    Some(s) => s,
                    None => {
                        start_time.set(Some(frame_time));
                        frame_time
                    }
                };

                let duration_us = ENTER_DURATION.as_micros() as f64;
                let progress = ((frame_time - start) as f64 / duration_us).clamp(0.0, 1.0);
                let eased = ease_out_cubic(progress);

                let offset = top_offset as f64 + (1.0 - eased) * ENTRY_OFFSET_PX;
                current_top_offset.set(offset);
                animated_window.set_margin(Edge::Top, SCREEN_MARGIN + offset.round() as i32);
                animated_window.set_opacity(eased);

                if progress >= 1.0 {
                    glib::ControlFlow::Break
                } else {
                    glib::ControlFlow::Continue
                }
            });
        }

        NotificationWindow {
            window,
            id: notification.id,
            current_top_offset,
            animation_epoch,
        }
    }

    /// Update the top margin for restacking.
    pub fn set_top_offset(&self, offset: i32) {
        let target_offset = offset as f64;
        let start_offset = self.current_top_offset.get();

        if (target_offset - start_offset).abs() < 0.5 {
            self.current_top_offset.set(target_offset);
            self.window
                .set_margin(Edge::Top, SCREEN_MARGIN + target_offset.round() as i32);
            self.window.set_opacity(1.0);
            return;
        }

        let window = self.window.clone();
        let animated_window = window.clone();
        let current_top_offset = self.current_top_offset.clone();
        let animation_epoch = self.animation_epoch.clone();
        let start_time = Rc::new(Cell::new(None::<i64>));
        let epoch = animation_epoch.get().wrapping_add(1);
        animation_epoch.set(epoch);

        window.add_tick_callback(move |_widget, frame_clock| {
            if animation_epoch.get() != epoch {
                return glib::ControlFlow::Break;
            }

            let frame_time = frame_clock.frame_time();
            let start = match start_time.get() {
                Some(s) => s,
                None => {
                    start_time.set(Some(frame_time));
                    frame_time
                }
            };

            let duration_us = RESTACK_DURATION.as_micros() as f64;
            let progress = ((frame_time - start) as f64 / duration_us).clamp(0.0, 1.0);
            let eased = ease_out_cubic(progress);
            let new_offset = start_offset + (target_offset - start_offset) * eased;

            current_top_offset.set(new_offset);
            animated_window.set_margin(Edge::Top, SCREEN_MARGIN + new_offset.round() as i32);
            animated_window.set_opacity(1.0);

            if progress >= 1.0 {
                glib::ControlFlow::Break
            } else {
                glib::ControlFlow::Continue
            }
        });
    }

    /// Best-effort measured height for stack layout.
    pub fn measured_height(&self) -> i32 {
        if let Some(child) = self.window.child() {
            let for_size = {
                let w = self.window.width();
                if w > 0 {
                    w
                } else {
                    -1
                }
            };
            let (_, natural, _, _) = child.measure(Orientation::Vertical, for_size);
            natural
        } else {
            self.window.height()
        }
    }

    /// Fade out and then close the underlying GTK window.
    ///
    /// The `NotificationWindow` is consumed — it has already been removed
    /// from the logical state so the animation is fire-and-forget.
    pub fn fade_out_and_destroy(self) {
        let window = self.window;
        let animated_window = window.clone();
        let close_window = window.clone();
        let current_top_offset = self.current_top_offset;
        let animation_epoch = self.animation_epoch;
        let start_opacity = window.opacity();
        let start_offset = current_top_offset.get();
        let target_offset = start_offset + ENTRY_OFFSET_PX;
        let start_time = Rc::new(Cell::new(None::<i64>));

        let epoch = animation_epoch.get().wrapping_add(1);
        animation_epoch.set(epoch);

        window.add_tick_callback(move |_widget, frame_clock| {
            if animation_epoch.get() != epoch {
                return glib::ControlFlow::Break;
            }

            let frame_time = frame_clock.frame_time();
            let start = match start_time.get() {
                Some(s) => s,
                None => {
                    start_time.set(Some(frame_time));
                    frame_time
                }
            };

            let duration_us = EXIT_DURATION.as_micros() as f64;
            let progress = ((frame_time - start) as f64 / duration_us).clamp(0.0, 1.0);
            let eased = ease_in_cubic(progress);

            let opacity = (start_opacity * (1.0 - eased)).clamp(0.0, 1.0);
            let offset = start_offset + (target_offset - start_offset) * eased;
            current_top_offset.set(offset);

            animated_window.set_margin(Edge::Top, SCREEN_MARGIN + offset.round() as i32);
            animated_window.set_opacity(opacity);

            if progress >= 1.0 {
                close_window.close();
                glib::ControlFlow::Break
            } else {
                glib::ControlFlow::Continue
            }
        });
    }
}
