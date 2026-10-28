//! Runs the engine's timers on the GLib main loop. A timer costs nothing
//! until it fires; there is no polling.

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use gtk4::glib::{self, SourceId};
use lucent::engine::{Effect, Event, Timer};
use tokio::sync::mpsc::UnboundedSender;

pub struct Timers {
    events: UnboundedSender<Event>,
    pending: Rc<RefCell<HashMap<Timer, SourceId>>>,
}

impl Timers {
    pub fn new(events: UnboundedSender<Event>) -> Self {
        Self {
            events,
            pending: Rc::default(),
        }
    }

    /// Apply a timer effect; other effects are ignored.
    pub fn apply(&self, effect: &Effect) {
        match *effect {
            Effect::ScheduleTimer { timer, after } => {
                self.cancel(timer);
                let events = self.events.clone();
                let pending = Rc::downgrade(&self.pending);
                let source = glib::timeout_add_local_once(after, move || {
                    // The source is finished once this runs; forget it so
                    // it is never removed twice.
                    if let Some(pending) = pending.upgrade() {
                        pending.borrow_mut().remove(&timer);
                    }
                    let _ = events.send(Event::TimerFired(timer));
                });
                self.pending.borrow_mut().insert(timer, source);
            }
            Effect::CancelTimer(timer) => self.cancel(timer),
            _ => {}
        }
    }

    fn cancel(&self, timer: Timer) {
        if let Some(source) = self.pending.borrow_mut().remove(&timer) {
            source.remove();
        }
    }
}
