//! The engine on the GTK main loop, wired to the adapters that apply its
//! effects, and answering the CLI's [`Command`]s.

use std::cell::RefCell;
use std::rc::Rc;
use std::time::{Instant, SystemTime};

use lucent::engine::{Effect, Engine, Event};
use tokio::sync::mpsc::UnboundedSender;

use crate::dbus::control::{Command, HistoryItem};
use crate::{dbus, timers, ui};

/// The engine and the places its effects go. Lives on the GTK main thread.
pub struct Core {
    engine: RefCell<Engine>,
    ui: Rc<ui::Ui>,
    timers: timers::Timers,
    /// Effects for the D-Bus thread (signals).
    signals: UnboundedSender<Effect>,
}

impl Core {
    pub fn new(
        engine: Engine,
        ui: Rc<ui::Ui>,
        timers: timers::Timers,
        signals: UnboundedSender<Effect>,
    ) -> Self {
        Self {
            engine: RefCell::new(engine),
            ui,
            timers,
            signals,
        }
    }

    /// Feed one event to the engine and apply its effects, in order.
    /// Returns the effects so a [`Command`] can answer from them.
    pub fn handle(&self, event: Event) -> Vec<Effect> {
        let effects = self.engine.borrow_mut().handle(event);
        for effect in &effects {
            self.ui.apply(effect);
            self.timers.apply(effect);
            if dbus::handles(effect) {
                let _ = self.signals.send(effect.clone());
            }
        }
        effects
    }

    /// Carry out a CLI command and send its answer. A dropped reply means
    /// the caller hung up, which is fine.
    pub fn answer(&self, command: Command) {
        match command {
            Command::Dismiss { all, reply } => {
                let event = if all {
                    Some(Event::UserDismissedAll)
                } else {
                    let newest = self.engine.borrow().newest_popup();
                    newest.map(Event::UserDismissed)
                };
                let dismissed = event.map_or(0, |event| {
                    self.handle(event)
                        .iter()
                        .filter(|e| matches!(e, Effect::NotificationClosed { .. }))
                        .count()
                });
                let _ = reply.send(dismissed as u32);
            }
            Command::Restore { id, reply } => {
                let mut items = self.history_items().into_iter();
                let entry = match id {
                    None => items.next(),
                    Some(id) => items.find(|item| item.id == id),
                };
                if entry.is_some() {
                    self.handle(Event::HistoryRestored(id));
                }
                let _ = reply.send(entry);
            }
            Command::ClearHistory(reply) => {
                let cleared = self.engine.borrow().status().history_count;
                self.handle(Event::HistoryCleared);
                let _ = reply.send(cleared as u32);
            }
            Command::History(reply) => {
                let _ = reply.send(self.history_items());
            }
        }
    }

    /// History, newest first, as the CLI sees it.
    fn history_items(&self) -> Vec<HistoryItem> {
        let (now, wall_now) = (Instant::now(), SystemTime::now());
        self.engine
            .borrow()
            .history()
            .map(|entry| HistoryItem::new(entry, now, wall_now))
            .collect()
    }
}
