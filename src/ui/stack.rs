//! The Popup stack: one transparent layer-shell window holding every card.
//!
//! Sharing one window is what lets the stack animate as a whole: cards
//! slide in, fade out and glide into place on the frame clock, placed by
//! transforms inside a [`StackView`]. The window is padded so card shadows
//! aren't clipped, and its input region is cut to the cards so clicks on
//! the shadow and the gaps fall through to whatever is underneath.
//!
//! While Notifications wait in Overflow, the "+N more" [`Pill`] sits one
//! gap past the oldest card. The window goes on the configured output, or
//! wherever the compositor puts it (the focused output) when none is set
//! or the named one isn't connected.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use gtk4::prelude::*;
use gtk4::{cairo, gdk, glib};
use gtk4_layer_shell::{Edge, KeyboardMode, Layer, LayerShell};
use lucent::config::{Horizontal, Vertical};
use lucent::engine::{Event, NotificationId, Popup};
use tokio::sync::mpsc::UnboundedSender;

use super::card::Card;
use super::layout;
use super::motion::{ENTER, ENTER_SCALE, LEAVE, LEAVE_SCALE, REFLOW, Tween};
use super::pill::Pill;
use super::theme::{Insets, Look, Theme};
use super::widgets::{Placement, StackView};

/// The layer-shell namespace compositors match for blur rules.
const NAMESPACE: &str = "lucent-notification";

pub struct Stack {
    inner: Rc<Inner>,
}

struct Inner {
    window: gtk4::Window,
    view: StackView,
    look: RefCell<Look>,
    padding: Cell<Insets>,
    /// Newest (nearest the anchored edge) first. Leaving cards keep their
    /// place until their animation ends.
    slots: RefCell<Vec<Slot>>,
    /// Window height while an animation runs; it only grows meanwhile, so
    /// the surface isn't resized every frame.
    height: Cell<i32>,
    ticking: Cell<bool>,
    pill: RefCell<PillSlot>,
    /// The output named in the config, if any.
    output: RefCell<Option<String>>,
    /// The monitor the window is pinned to; `None` follows focus.
    monitor: RefCell<Option<gdk::Monitor>>,
}

/// The Overflow pill's place and fade. It stays in the view, transparent
/// and out of the input region, while nothing waits.
struct PillSlot {
    pill: Pill,
    /// Notifications waiting in Overflow.
    count: usize,
    height: i32,
    /// Distance from the anchored edge.
    offset: Tween,
    opacity: Tween,
}

struct Slot {
    card: Card,
    leaving: bool,
    height: i32,
    /// Distance from the anchored edge.
    offset: Tween,
    dx: Tween,
    dy: Tween,
    scale: Tween,
    opacity: Tween,
}

impl Stack {
    pub fn new(
        look: &Look,
        theme: &Theme,
        output: Option<String>,
        events: &UnboundedSender<Event>,
    ) -> Self {
        let window = gtk4::Window::new();
        window.add_css_class("lucent-stack");
        window.set_decorated(false);
        window.set_resizable(false);
        window.init_layer_shell();
        window.set_layer(Layer::Overlay);
        window.set_namespace(Some(NAMESPACE));
        window.set_keyboard_mode(KeyboardMode::None);
        window.set_exclusive_zone(0);

        let view = StackView::new(look.width);
        window.set_child(Some(&view));
        // Added first, so cards draw over it while they glide past.
        let pill = Pill::new(look.width, events);
        pill.widget().set_opacity(0.0);
        pill.widget().set_can_target(false);
        view.add(
            pill.widget(),
            Placement {
                x: 0.0,
                y: 0.0,
                scale: 1.0,
            },
        );

        let inner = Rc::new(Inner {
            window,
            view,
            look: RefCell::new(look.clone()),
            padding: Cell::new(Insets::default()),
            slots: RefCell::default(),
            height: Cell::new(0),
            ticking: Cell::new(false),
            pill: RefCell::new(PillSlot {
                pill,
                count: 0,
                height: 0,
                offset: Tween::settled(0.0),
                opacity: Tween::settled(0.0),
            }),
            output: RefCell::new(output),
            monitor: RefCell::default(),
        });
        inner.place_window(look, theme);
        inner.pick_monitor();
        if let Some(display) = gdk::Display::default() {
            // Follow outputs being plugged in and out.
            let weak = Rc::downgrade(&inner);
            display.monitors().connect_items_changed(move |_, _, _, _| {
                if let Some(inner) = weak.upgrade() {
                    inner.pick_monitor();
                }
            });
        }
        {
            let weak = Rc::downgrade(&inner);
            inner.window.connect_map(move |_| {
                if let Some(inner) = weak.upgrade() {
                    inner.frame(glib::monotonic_time());
                }
            });
        }
        Self { inner }
    }

    /// Use new sizes and placement. Cards already on screen take the new
    /// width; their contents change when next replaced.
    pub fn set_look(&self, look: &Look, theme: &Theme) {
        *self.inner.look.borrow_mut() = look.clone();
        self.inner.place_window(look, theme);
        self.inner.view.set_child_width(look.width);
        for slot in self.inner.slots.borrow().iter() {
            slot.card.widget().set_size_request(look.width, -1);
        }
        self.inner.pill.borrow().pill.set_width(look.width);
        self.inner.relayout();
    }

    /// Pin the stack to the output with this connector name (`DP-1`), or
    /// let it follow focus with `None`.
    pub fn set_output(&self, output: Option<String>) {
        *self.inner.output.borrow_mut() = output;
        self.inner.pick_monitor();
    }

    /// Show the "+N more" pill for `count` waiting Notifications, or fade
    /// it out at zero.
    pub fn set_overflow(&self, count: usize) {
        let inner = &self.inner;
        let now = glib::monotonic_time();
        let reduced = reduced_motion();
        {
            let mut slot = inner.pill.borrow_mut();
            let was = std::mem::replace(&mut slot.count, count);
            slot.pill.set_count(count);
            slot.pill.widget().set_can_target(count > 0);
            match (was > 0, count > 0) {
                (false, true) => {
                    slot.opacity = slot.opacity.retarget(1.0, now, ENTER, reduced);
                }
                (true, false) => {
                    slot.opacity = slot.opacity.retarget(0.0, now, LEAVE, reduced);
                }
                _ => {}
            }
        }
        if count > 0 {
            inner.window.set_visible(true);
        }
        inner.relayout();
    }

    /// Slide a new card in at the anchored edge.
    pub fn show(&self, card: Card) {
        let inner = &self.inner;
        let now = glib::monotonic_time();
        let reduced = reduced_motion();
        let (ex, ey) = layout::enter_offset(inner.look.borrow().position);
        inner.view.add(
            card.widget(),
            Placement {
                x: 0.0,
                y: 0.0,
                scale: ENTER_SCALE as f32,
            },
        );
        card.widget().set_opacity(0.0);
        inner.slots.borrow_mut().insert(
            0,
            Slot {
                height: inner.view.child_height(card.widget()),
                card,
                leaving: false,
                offset: Tween::settled(0.0),
                dx: Tween::new(ex, 0.0, now, ENTER, reduced),
                dy: Tween::new(ey, 0.0, now, ENTER, reduced),
                scale: Tween::new(ENTER_SCALE, 1.0, now, ENTER, reduced),
                opacity: Tween::new(0.0, 1.0, now, ENTER, reduced),
            },
        );
        inner.window.set_visible(true);
        inner.relayout();
    }

    /// Fade a card out; the rest of the stack glides into its place.
    pub fn remove(&self, id: NotificationId) {
        let inner = &self.inner;
        let now = glib::monotonic_time();
        let reduced = reduced_motion();
        let (ex, ey) = layout::enter_offset(inner.look.borrow().position);
        if let Some(slot) = inner
            .slots
            .borrow_mut()
            .iter_mut()
            .find(|s| s.card.id == id && !s.leaving)
        {
            slot.leaving = true;
            slot.dx = slot.dx.retarget(ex, now, LEAVE, reduced);
            slot.dy = slot.dy.retarget(ey, now, LEAVE, reduced);
            slot.scale = slot.scale.retarget(LEAVE_SCALE, now, LEAVE, reduced);
            slot.opacity = slot.opacity.retarget(0.0, now, LEAVE, reduced);
        }
        inner.relayout();
    }

    /// Show a replaced Notification in its card, in place.
    pub fn update(&self, popup: &Popup) {
        self.with_card(popup.id(), |card| card.update(popup));
        self.inner.relayout();
    }

    /// Run `f` on the visible card for `id`, if there is one.
    pub fn with_card(&self, id: NotificationId, f: impl FnOnce(&Card)) {
        if let Some(slot) = self
            .inner
            .slots
            .borrow()
            .iter()
            .find(|s| s.card.id == id && !s.leaving)
        {
            f(&slot.card);
        }
    }
}

impl Inner {
    /// Anchor the window and pad it for the Theme's shadows.
    fn place_window(&self, look: &Look, theme: &Theme) {
        let position = look.position;
        let padding = layout::padding(position, theme.shadow_reach(), look.margin);
        let margins = layout::window_margins(position, padding, look.margin);
        self.padding.set(padding);
        let window = &self.window;
        window.set_anchor(Edge::Top, position.vertical == Vertical::Top);
        window.set_anchor(Edge::Bottom, position.vertical == Vertical::Bottom);
        window.set_anchor(Edge::Left, position.horizontal == Horizontal::Left);
        window.set_anchor(Edge::Right, position.horizontal == Horizontal::Right);
        window.set_margin(Edge::Top, margins.top);
        window.set_margin(Edge::Bottom, margins.bottom);
        window.set_margin(Edge::Left, margins.left);
        window.set_margin(Edge::Right, margins.right);
    }

    /// Put the window on the configured output, or let the compositor
    /// choose when none is set or it isn't connected.
    fn pick_monitor(&self) {
        let monitors: Vec<gdk::Monitor> = gdk::Display::default()
            .map(|display| {
                let list = display.monitors();
                (0..list.n_items())
                    .filter_map(|i| list.item(i)?.downcast::<gdk::Monitor>().ok())
                    .collect()
            })
            .unwrap_or_default();
        let connectors: Vec<Option<String>> = monitors
            .iter()
            .map(|m| m.connector().map(String::from))
            .collect();
        let wanted = self.output.borrow().clone();
        let picked =
            layout::pick_output(wanted.as_deref(), &connectors).map(|i| monitors[i].clone());
        if let (Some(name), None) = (&wanted, &picked) {
            tracing::warn!(
                output = %name,
                connected = ?connectors.iter().flatten().collect::<Vec<_>>(),
                "output isn't connected; Popups follow the focused output"
            );
        }
        if *self.monitor.borrow() != picked {
            self.window.set_monitor(picked.as_ref());
            *self.monitor.borrow_mut() = picked;
        }
    }

    /// Measure the cards, give each its target place and start animating.
    fn relayout(self: &Rc<Self>) {
        let now = glib::monotonic_time();
        let reduced = reduced_motion();
        let gap = self.look.borrow().gap;
        {
            let mut slots = self.slots.borrow_mut();
            for slot in slots.iter_mut().filter(|s| !s.leaving) {
                slot.height = self.view.child_height(slot.card.widget());
            }
            let heights: Vec<i32> = slots
                .iter()
                .filter(|s| !s.leaving)
                .map(|s| s.height)
                .collect();
            let targets = layout::offsets(&heights, gap);
            for (slot, target) in slots.iter_mut().filter(|s| !s.leaving).zip(targets) {
                let target = f64::from(target);
                if slot.offset.target() != target {
                    slot.offset = slot.offset.retarget(target, now, REFLOW, reduced);
                }
            }

            let mut pill = self.pill.borrow_mut();
            pill.height = self.view.child_height(pill.pill.widget());
            let target = f64::from(layout::pill_offset(&heights, gap));
            let unseen = pill.opacity.value(now) <= 0.0 && pill.opacity.target() > 0.0;
            if unseen {
                // Appearing: fade in where it belongs instead of gliding.
                pill.offset = Tween::settled(target);
            } else if pill.offset.target() != target {
                pill.offset = pill.offset.retarget(target, now, REFLOW, reduced);
            }
        }
        self.frame(now);
        self.start_ticking();
    }

    fn start_ticking(self: &Rc<Self>) {
        if self.ticking.replace(true) {
            return;
        }
        let weak = Rc::downgrade(self);
        self.view.add_tick_callback(move |_, clock| {
            let Some(inner) = weak.upgrade() else {
                return glib::ControlFlow::Break;
            };
            if inner.frame(clock.frame_time()) {
                glib::ControlFlow::Continue
            } else {
                inner.ticking.set(false);
                glib::ControlFlow::Break
            }
        });
    }

    /// Place every card for frame time `now`. Returns whether anything is
    /// still moving.
    fn frame(&self, now: i64) -> bool {
        let look = self.look.borrow();
        let pad = self.padding.get();
        let mut slots = self.slots.borrow_mut();

        let finished: Vec<usize> = (0..slots.len())
            .filter(|&i| slots[i].leaving && slots[i].opacity.done(now))
            .collect();
        for i in finished.into_iter().rev() {
            let slot = slots.remove(i);
            self.view.remove(slot.card.widget());
        }

        let pill = self.pill.borrow();
        let pill_shown = pill.count > 0 || pill.opacity.value(now) > 0.0;
        let animating = slots.iter().any(|s| {
            [&s.offset, &s.dx, &s.dy, &s.scale, &s.opacity]
                .iter()
                .any(|t| !t.done(now))
        }) || !pill.offset.done(now)
            || !pill.opacity.done(now);
        let reach = |offset: &Tween, height: i32| {
            offset.value(now).max(offset.target()).ceil() as i32 + height
        };
        let extent = slots
            .iter()
            .map(|s| reach(&s.offset, s.height))
            .chain(pill_shown.then(|| reach(&pill.offset, pill.height)))
            .max()
            .unwrap_or(0);
        let needed = pad.top + extent + pad.bottom;
        let height = if animating {
            needed.max(self.height.get())
        } else {
            needed
        };
        self.height.set(height);
        let width = pad.left + look.width + pad.right;
        self.view.set_size(width, height);

        // Top edge of something `offset` from the anchored edge.
        let top = |offset: f64, item_height: i32| {
            let offset = offset as f32;
            match look.position.vertical {
                Vertical::Top => pad.top as f32 + offset,
                Vertical::Bottom => (height - pad.bottom - item_height) as f32 - offset,
            }
        };

        let mut cards = Vec::new();
        for slot in slots.iter() {
            let top = top(slot.offset.value(now), slot.height);
            let place = Placement {
                x: pad.left as f32 + slot.dx.value(now) as f32,
                y: top + slot.dy.value(now) as f32,
                scale: slot.scale.value(now) as f32,
            };
            self.view.place(slot.card.widget(), place);
            slot.card
                .widget()
                .set_opacity(slot.opacity.value(now).clamp(0.0, 1.0));
            if !slot.leaving {
                cards.push(cairo::RectangleInt::new(
                    place.x.round() as i32,
                    place.y.round() as i32,
                    look.width,
                    slot.height,
                ));
            }
        }
        let pill_top = top(pill.offset.value(now), pill.height);
        self.view.place(
            pill.pill.widget(),
            Placement {
                x: pad.left as f32,
                y: pill_top,
                scale: 1.0,
            },
        );
        pill.pill
            .widget()
            .set_opacity(pill.opacity.value(now).clamp(0.0, 1.0));
        if pill.count > 0 {
            let pill_width = pill.pill.natural_width().min(look.width);
            cards.push(cairo::RectangleInt::new(
                pad.left + (look.width - pill_width) / 2,
                pill_top.round() as i32,
                pill_width,
                pill.height,
            ));
        }
        self.set_input_region(&cards);

        if slots.is_empty() && !pill_shown && !animating {
            self.window.set_visible(false);
        }
        animating
    }

    /// Only the cards take pointer input; the shadows and gaps don't.
    fn set_input_region(&self, cards: &[cairo::RectangleInt]) {
        if let Some(surface) = self.window.surface() {
            surface.set_input_region(Some(&cairo::Region::create_rectangles(cards)));
        }
    }
}

/// Whether the desktop asks for less motion.
fn reduced_motion() -> bool {
    gtk4::Settings::default().is_some_and(|settings| {
        !settings.is_gtk_enable_animations()
            || settings.gtk_interface_reduced_motion() == gtk4::ReducedMotion::Reduce
    })
}
