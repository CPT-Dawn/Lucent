//! GTK adapter: draws the Popups the engine asks for and turns clicks into
//! engine events. All state lives on the GTK main thread.

pub mod ago;
pub mod countdown;
pub mod icon;
pub mod layout;
pub mod markup;
pub mod motion;
pub mod style;
pub mod theme;
pub mod window;

use std::cell::RefCell;
use std::rc::{Rc, Weak};

use gtk4::glib;
use lucent::engine::{Effect, Event, NotificationId, Popup};
use tokio::sync::mpsc::UnboundedSender;

use crate::config::Config;
use window::NotificationWindow;

/// Vertical gap between stacked Popups.
const STACK_GAP: i32 = 10;
/// Height assumed for a Popup that hasn't been measured yet.
const FALLBACK_HEIGHT: i32 = 90;

/// The Popups on screen.
pub struct Ui {
    config: Rc<Config>,
    events: UnboundedSender<Event>,
    /// Popup windows, oldest (top) first.
    windows: Rc<RefCell<Vec<NotificationWindow>>>,
}

impl Ui {
    pub fn new(config: Rc<Config>, events: UnboundedSender<Event>) -> Self {
        Self {
            config,
            events,
            windows: Rc::default(),
        }
    }

    /// Apply one engine effect. Effects that aren't about Popups are
    /// ignored.
    pub fn apply(&self, effect: &Effect) {
        match effect {
            Effect::ShowPopup(popup) => self.show(popup),
            Effect::UpdatePopup(popup) => self.update(popup),
            Effect::RemovePopup { id, reason } => {
                tracing::debug!(id, reason = reason.code(), "removing Popup");
                self.remove(*id);
            }
            _ => {}
        }
    }

    fn show(&self, popup: &Popup) {
        let n = &popup.notification;
        tracing::info!(id = n.id, app = %n.app_name, summary = %n.summary, "showing Popup");
        let top = stack_height(&self.windows.borrow());
        let window = NotificationWindow::new(popup, &self.config, top, &self.events);
        self.windows.borrow_mut().push(window);
        self.restack();
    }

    /// Rebuild a visible Popup with new contents, keeping its place.
    fn update(&self, popup: &Popup) {
        let mut windows = self.windows.borrow_mut();
        let Some(index) = windows.iter().position(|w| w.id == popup.id()) else {
            return;
        };
        let top = stack_height(&windows[..index]);
        let fresh = NotificationWindow::new(popup, &self.config, top, &self.events);
        std::mem::replace(&mut windows[index], fresh).fade_out_and_destroy();
        drop(windows);
        self.restack();
    }

    fn remove(&self, id: NotificationId) {
        let mut windows = self.windows.borrow_mut();
        if let Some(index) = windows.iter().position(|w| w.id == id) {
            windows.remove(index).fade_out_and_destroy();
        }
        drop(windows);
        self.restack();
    }

    /// Move every Popup to its slot now, and once more after GTK has laid
    /// out new windows and their heights are known.
    fn restack(&self) {
        restack(&self.windows.borrow());
        let windows: Weak<_> = Rc::downgrade(&self.windows);
        glib::idle_add_local_once(move || {
            if let Some(windows) = windows.upgrade() {
                restack(&windows.borrow());
            }
        });
    }
}

fn restack(windows: &[NotificationWindow]) {
    let mut offset = 0;
    for window in windows {
        window.set_top_offset(offset);
        offset += height(window) + STACK_GAP;
    }
}

fn stack_height(windows: &[NotificationWindow]) -> i32 {
    windows.iter().map(|w| height(w) + STACK_GAP).sum()
}

fn height(window: &NotificationWindow) -> i32 {
    match window.measured_height() {
        h if h > 0 => h,
        _ => FALLBACK_HEIGHT,
    }
}
