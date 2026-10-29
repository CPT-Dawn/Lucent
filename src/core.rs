//! The Notification core: a pure state machine with no Wayland or D-Bus inside.
//!
//! Adapters translate the outside world into [`Event`]s, feed them to
//! [`Core::handle`], and carry out the returned [`Effect`]s in order. This is
//! Lucent's main test seam.

use std::collections::{HashMap, VecDeque};
use std::time::{Duration, Instant};

use crate::config::{ClickAction, Config};

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
    pub actions: Vec<Action>,
}

/// A labelled choice the sender offers; the key is reported back when it is
/// invoked. The key `default` marks the Default action.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Action {
    pub key: String,
    pub label: String,
}

/// The key that marks an Action as the Default action.
pub const DEFAULT_ACTION: &str = "default";

/// A pointer button pressed on a Popup.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Button {
    Left,
    Middle,
    Right,
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
    /// Dismissed by the user.
    Dismissed = 2,
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
    /// The user pressed `button` on this Notification's Popup.
    Click {
        id: u32,
        button: Button,
    },
    /// The pointer moved onto this Notification's Popup.
    HoverStart(u32),
    /// The pointer left this Notification's Popup.
    HoverEnd(u32),
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
    /// Emit `ActionInvoked` for `id` with the Action's `key`.
    ActionInvoked { id: u32, key: String },
}

/// Where a visible Popup's timeout stands.
#[derive(Debug, Clone, Copy)]
enum Timeout {
    /// A timer started at `started` fires after `after`.
    Running { started: Instant, after: Duration },
    /// Hovered: no timer runs, and `remaining` is left once the pointer goes.
    Paused { remaining: Duration },
}

/// The time source for hover pauses; tests swap in one they move by hand.
struct Clock(Box<dyn Fn() -> Instant>);

impl std::fmt::Debug for Clock {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Clock")
    }
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
    /// The Actions of every live Notification that offers any.
    actions: HashMap<u32, Vec<Action>>,
    /// The timeout of every Popup in the Stack.
    timeouts: HashMap<u32, Timeout>,
    clock: Clock,
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
            actions: HashMap::new(),
            timeouts: HashMap::new(),
            clock: Clock(Box::new(Instant::now)),
        }
    }

    /// Reads the time from `clock` instead of the system clock.
    pub fn with_clock(mut self, clock: impl Fn() -> Instant + 'static) -> Self {
        self.clock = Clock(Box::new(clock));
        self
    }

    /// The defaults, except the Stack shows at most `max_visible` Popups.
    pub fn with_max_visible(max_visible: usize) -> Self {
        let mut config = Config::default();
        config.general.max_visible = max_visible;
        Self::with_config(config)
    }

    pub fn handle(&mut self, event: Event) -> Vec<Effect> {
        match event {
            Event::Notify(n) => self.notify(n),
            Event::TimerFired(id) => self.expire(id),
            Event::Close(id) => self.close(id),
            Event::Click { id, button } => self.click(id, button),
            Event::HoverStart(id) => self.hover_start(id),
            Event::HoverEnd(id) => self.hover_end(id),
        }
    }

    fn notify(&mut self, n: Notification) -> Vec<Effect> {
        if let Some(effects) = self.replace(&n) {
            return effects;
        }
        self.last_id = self.last_id.wrapping_add(1).max(1);
        let id = self.last_id;
        self.set_actions(id, n.actions);
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
        let mut effects = vec![Effect::Accepted(id), self.stack_effect()];
        effects.extend(self.start_timer(id, self.config.general.timeout));
        effects
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
            self.set_actions(id, n.actions.clone());
            let mut effects = vec![Effect::Accepted(id), self.stack_effect()];
            effects.extend(self.start_timer(id, self.config.general.timeout));
            return Some(effects);
        }
        let queued = self.queue.iter_mut().find(|p| p.id == id)?;
        update(queued);
        self.set_actions(id, n.actions.clone());
        Some(vec![Effect::Accepted(id)])
    }

    fn expire(&mut self, id: u32) -> Vec<Effect> {
        // A paused timeout has no timer; one that fired anyway is stale.
        if !matches!(self.timeouts.get(&id), Some(Timeout::Running { .. })) {
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

    fn click(&mut self, id: u32, button: Button) -> Vec<Effect> {
        if !self.stack.iter().any(|p| p.id == id) {
            return vec![];
        }
        let general = &self.config.general;
        let action = match button {
            Button::Left => general.left_click,
            Button::Middle => general.middle_click,
            Button::Right => general.right_click,
        };
        let mut effects = vec![Effect::CancelTimer(id)];
        match action {
            ClickAction::InvokeDefaultAction => {
                let has_default = self
                    .actions
                    .get(&id)
                    .is_some_and(|a| a.iter().any(|a| a.key == DEFAULT_ACTION));
                if has_default {
                    effects.push(Effect::ActionInvoked {
                        id,
                        key: DEFAULT_ACTION.into(),
                    });
                }
            }
            ClickAction::Dismiss => {}
            ClickAction::DismissAll => return self.dismiss_all(),
            ClickAction::None => return vec![],
        }
        self.remove(id, CloseReason::Dismissed, effects)
    }

    /// Starts `id`'s timeout, or, while it is hovered, only resets what will
    /// be left once the pointer goes.
    fn start_timer(&mut self, id: u32, after: Duration) -> Option<Effect> {
        if let Some(Timeout::Paused { remaining }) = self.timeouts.get_mut(&id) {
            *remaining = after;
            return None;
        }
        let started = (self.clock.0)();
        self.timeouts
            .insert(id, Timeout::Running { started, after });
        Some(Effect::StartTimer { id, after })
    }

    fn hover_start(&mut self, id: u32) -> Vec<Effect> {
        let Some(&Timeout::Running { started, after }) = self.timeouts.get(&id) else {
            return vec![];
        };
        let shown = (self.clock.0)().saturating_duration_since(started);
        let remaining = after.saturating_sub(shown);
        self.timeouts.insert(id, Timeout::Paused { remaining });
        vec![Effect::CancelTimer(id)]
    }

    fn hover_end(&mut self, id: u32) -> Vec<Effect> {
        let Some(&Timeout::Paused { remaining }) = self.timeouts.get(&id) else {
            return vec![];
        };
        self.timeouts.remove(&id);
        self.start_timer(id, remaining).into_iter().collect()
    }

    /// Empties the Stack and the Queue in one change, reporting each closed.
    fn dismiss_all(&mut self) -> Vec<Effect> {
        let stack = std::mem::take(&mut self.stack);
        let queue = std::mem::take(&mut self.queue);
        self.actions.clear();
        self.timeouts.clear();
        let mut effects: Vec<Effect> = stack.iter().map(|p| Effect::CancelTimer(p.id)).collect();
        effects.push(self.stack_effect());
        effects.extend(stack.iter().chain(&queue).map(|p| Effect::Closed {
            id: p.id,
            reason: CloseReason::Dismissed,
        }));
        effects
    }

    fn set_actions(&mut self, id: u32, actions: Vec<Action>) {
        if actions.is_empty() {
            self.actions.remove(&id);
        } else {
            self.actions.insert(id, actions);
        }
    }

    /// Takes a live Notification out of the Stack or Queue, refills the Stack
    /// from the Queue, and reports it closed for `reason`.
    fn remove(&mut self, id: u32, reason: CloseReason, mut effects: Vec<Effect>) -> Vec<Effect> {
        self.stack.retain(|p| p.id != id);
        self.queue.retain(|p| p.id != id);
        self.actions.remove(&id);
        self.timeouts.remove(&id);
        let started = self.promote();
        effects.push(self.stack_effect());
        effects.extend(started);
        effects.push(Effect::Closed { id, reason });
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
            effects.extend(self.start_timer(popup.id, self.config.general.timeout));
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
