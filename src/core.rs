//! The Notification core: a pure state machine with no Wayland or D-Bus inside.
//!
//! Adapters translate the outside world into [`Event`]s, feed them to
//! [`Core::handle`], and carry out the returned [`Effect`]s in order. This is
//! Lucent's main test seam.

use std::collections::VecDeque;
use std::time::Duration;

use crate::config::Config;

/// A Notification as an application asked for it, already stripped of any
/// transport types.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Notification {
    pub app_name: String,
    pub summary: String,
    pub body: String,
    /// The id of a live Notification this one should update in place; 0 (or
    /// any id that matches nothing) asks for a fresh one.
    pub replaces_id: u32,
}

/// What the renderer should show for one Notification.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Popup {
    pub id: u32,
    pub app_name: String,
    pub summary: String,
    pub body: String,
    /// Notifications waiting in the Queue, shown as a "+N more" hint. Only the
    /// last Popup of the Stack carries a non-zero count.
    pub more: usize,
}

/// Why a Notification closed, as numbered by the freedesktop spec.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u32)]
pub enum CloseReason {
    Expired = 1,
    /// Closed by a `CloseNotification` call.
    Closed = 3,
}

/// Something that happened outside the core.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Event {
    Notify(Notification),
    /// The timer started by [`Effect::StartTimer`] for this id has elapsed.
    TimerFired(u32),
    /// An application asked for this Notification to close.
    Close(u32),
    /// Lucent has just started serving, with nothing to show yet.
    Started,
    /// The timer started by [`Effect::StartIdleTimer`] has elapsed.
    IdleTimerFired,
}

/// Something the adapters must do, in the order given.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Effect {
    /// The Notify was accepted under this id; reply to the sender with it.
    Accepted(u32),
    /// The Stack changed; this is the complete set of Popups to show, in order.
    Stack(Vec<Popup>),
    /// Fire [`Event::TimerFired`] for `id` once `after` has elapsed.
    StartTimer { id: u32, after: Duration },
    /// Drop the timer for `id`, if one is running.
    CancelTimer(u32),
    /// Emit `NotificationClosed` for `id`.
    Closed { id: u32, reason: CloseReason },
    /// The Stack and Queue are empty: fire [`Event::IdleTimerFired`] once
    /// `after` has elapsed (Idle exit, ADR-0002).
    StartIdleTimer { after: Duration },
    /// Drop the idle timer: there is something to show again.
    CancelIdleTimer,
    /// Release the bus name and end the process.
    Exit,
}

#[derive(Debug)]
pub struct Core {
    config: Config,
    last_id: u32,
    /// How many Popups the Stack holds before Notifications wait in the
    /// Queue: `[general] max_visible`, but at least one.
    max_visible: usize,
    /// Visible Popups, nearest the anchor first. `more` is filled in only
    /// when the Stack is handed out.
    stack: Vec<Popup>,
    /// Notifications waiting for room, oldest first.
    queue: VecDeque<Popup>,
    /// Whether the idle grace period is running.
    idle_armed: bool,
}

impl Default for Core {
    fn default() -> Self {
        Self::with_config(Config::default())
    }
}

impl Core {
    /// A core running on the built-in defaults.
    pub fn new() -> Self {
        Self::default()
    }

    /// A core running on the user's resolved config.
    pub fn with_config(config: Config) -> Self {
        Self {
            max_visible: config.general.max_visible.max(1),
            config,
            last_id: 0,
            stack: Vec::new(),
            queue: VecDeque::new(),
            idle_armed: false,
        }
    }

    /// The defaults, except the Stack shows at most `max_visible` Popups.
    pub fn with_max_visible(max_visible: usize) -> Self {
        let mut config = Config::default();
        config.general.max_visible = max_visible;
        Self::with_config(config)
    }

    /// Continues numbering after `last_id`, the last id an earlier run handed
    /// out, so ids keep increasing across Idle exits.
    pub fn resuming_after(mut self, last_id: u32) -> Self {
        self.last_id = last_id;
        self
    }

    /// The most recent id handed out, for the adapters to persist.
    pub fn last_id(&self) -> u32 {
        self.last_id
    }

    pub fn handle(&mut self, event: Event) -> Vec<Effect> {
        match event {
            Event::Notify(n) => {
                let mut effects = self.notify(n);
                if std::mem::take(&mut self.idle_armed) {
                    effects.push(Effect::CancelIdleTimer);
                }
                effects
            }
            Event::TimerFired(id) => self.expire(id),
            Event::Close(id) => self.close(id),
            Event::Started => self.arm_idle().into_iter().collect(),
            Event::IdleTimerFired => self.idle_elapsed(),
        }
    }

    /// Starts the grace period if idle exit is on and nothing is live.
    fn arm_idle(&mut self) -> Option<Effect> {
        if !self.config.general.idle_exit || !self.stack.is_empty() || !self.queue.is_empty() {
            return None;
        }
        self.idle_armed = true;
        Some(Effect::StartIdleTimer {
            after: self.config.general.idle_grace,
        })
    }

    /// Exits only if the grace period is still running: a Notification may
    /// have cancelled it after the timer had already fired.
    fn idle_elapsed(&mut self) -> Vec<Effect> {
        if std::mem::take(&mut self.idle_armed) {
            vec![Effect::Exit]
        } else {
            vec![]
        }
    }

    fn notify(&mut self, n: Notification) -> Vec<Effect> {
        if let Some(effects) = self.replace(&n) {
            return effects;
        }
        self.last_id = self.last_id.wrapping_add(1).max(1);
        let id = self.last_id;
        let popup = Popup {
            id,
            app_name: n.app_name,
            summary: n.summary,
            body: n.body,
            more: 0,
        };
        if self.stack.len() >= self.max_visible {
            self.queue.push_back(popup);
            return vec![Effect::Accepted(id), self.stack_effect()];
        }
        self.stack.insert(0, popup);
        vec![
            Effect::Accepted(id),
            self.stack_effect(),
            Effect::StartTimer {
                id,
                after: self.config.general.timeout,
            },
        ]
    }

    /// Updates the live Notification `n` replaces without moving it, or
    /// returns `None` if it replaces nothing that is still live.
    fn replace(&mut self, n: &Notification) -> Option<Vec<Effect>> {
        let id = n.replaces_id;
        let update = |p: &mut Popup| {
            p.app_name.clone_from(&n.app_name);
            p.summary.clone_from(&n.summary);
            p.body.clone_from(&n.body);
        };
        if let Some(popup) = self.stack.iter_mut().find(|p| p.id == id) {
            update(popup);
            return Some(vec![
                Effect::Accepted(id),
                self.stack_effect(),
                Effect::StartTimer {
                    id,
                    after: self.config.general.timeout,
                },
            ]);
        }
        let queued = self.queue.iter_mut().find(|p| p.id == id)?;
        update(queued);
        Some(vec![Effect::Accepted(id)])
    }

    fn expire(&mut self, id: u32) -> Vec<Effect> {
        if !self.stack.iter().any(|p| p.id == id) {
            return vec![];
        }
        self.remove(id, CloseReason::Expired, vec![])
    }

    fn close(&mut self, id: u32) -> Vec<Effect> {
        if self.stack.iter().any(|p| p.id == id) {
            self.remove(id, CloseReason::Closed, vec![Effect::CancelTimer(id)])
        } else if self.queue.iter().any(|p| p.id == id) {
            self.remove(id, CloseReason::Closed, vec![])
        } else {
            vec![]
        }
    }

    /// Takes a live Notification out of the Stack or Queue, refills the Stack
    /// from the Queue, and reports it closed for `reason`.
    fn remove(&mut self, id: u32, reason: CloseReason, mut effects: Vec<Effect>) -> Vec<Effect> {
        self.stack.retain(|p| p.id != id);
        self.queue.retain(|p| p.id != id);
        let started = self.promote();
        effects.push(self.stack_effect());
        effects.extend(started);
        effects.push(Effect::Closed { id, reason });
        effects.extend(self.arm_idle());
        effects
    }

    /// Moves queued Notifications into the Stack while there is room, as the
    /// newest Popups, and starts their timers.
    fn promote(&mut self) -> Vec<Effect> {
        let mut effects = Vec::new();
        while self.stack.len() < self.max_visible {
            let Some(popup) = self.queue.pop_front() else {
                break;
            };
            effects.push(Effect::StartTimer {
                id: popup.id,
                after: self.config.general.timeout,
            });
            self.stack.insert(0, popup);
        }
        effects
    }

    /// The Stack as the renderer should show it, "+N more" on the last slot.
    fn stack_effect(&self) -> Effect {
        let mut stack = self.stack.clone();
        if let Some(last) = stack.last_mut() {
            last.more = self.queue.len();
        }
        Effect::Stack(stack)
    }
}
