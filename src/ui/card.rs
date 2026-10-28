//! The D · Lucent Popup card.
//!
//! ```text
//! .popup (Overlay, clips to its rounded corners)
//! ├─ content (vertical Box, rebuilt when the Notification is replaced)
//! │  ├─ .top: media │ main
//! │  │   media: Overlay around the 42px tile, 5px larger on each side so the
//! │  │          app badge (and the progress ring, #7) can overhang it
//! │  │   main:  .head (.app · .time) / .summary / .body (3 lines)
//! │  └─ .actions: one button per Action but "default", the first .primary
//! ├─ button.close (top right, shown on hover)
//! └─ countdown line (bottom edge)
//! ```
//!
//! The card turns pointer input into engine events and draws what the
//! engine says; it holds no behaviour of its own.

use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::time::{Duration, Instant};

use gtk4::prelude::*;
use gtk4::{Align, Box as GtkBox, Label, Orientation, Overlay, gdk, gio, glib, pango};
use lucent::engine::{Action, Event, Notification, NotificationId, Popup, Urgency};
use tokio::sync::mpsc::UnboundedSender;

use super::ago::{ago, until_change};
use super::countdown::Countdown;
use super::icon::{self, Art, Source};
use super::markup::{body_markup, escape};
use super::theme::{Look, Rgba, Theme};
use super::widgets::{Fit, Tile};

/// Height of the countdown line.
const LINE_HEIGHT: i32 = 2;
/// Opacity of the countdown line.
const LINE_OPACITY: f64 = 0.7;

/// One Popup on screen.
pub struct Card {
    pub id: NotificationId,
    root: Overlay,
    shared: Rc<Shared>,
    /// For the Action buttons, rebuilt when the Notification is replaced.
    events: UnboundedSender<Event>,
}

/// What the card's signal handlers need.
struct Shared {
    notification: RefCell<Notification>,
    countdown: RefCell<Option<Countdown>>,
    ticking: Cell<bool>,
    line: gtk4::DrawingArea,
    look: Look,
    theme: Rc<RefCell<Theme>>,
}

impl Card {
    pub fn new(
        popup: &Popup,
        look: &Look,
        theme: &Rc<RefCell<Theme>>,
        events: &UnboundedSender<Event>,
    ) -> Self {
        let id = popup.id();
        let root = Overlay::new();
        root.add_css_class("popup");
        root.set_overflow(gtk4::Overflow::Hidden);
        root.set_size_request(look.width, -1);

        let close = gtk4::Button::from_icon_name("window-close-symbolic");
        close.add_css_class("close");
        close.set_tooltip_text(Some("Dismiss"));
        // The layer surface takes no keyboard input; a focused button
        // would only show the close button through `:focus-within`.
        close.set_can_focus(false);
        close.set_halign(Align::End);
        close.set_valign(Align::Start);
        close.set_margin_top(8);
        close.set_margin_end(8);
        {
            let events = events.clone();
            close.connect_clicked(move |_| send(&events, Event::UserDismissed(id)));
        }
        root.add_overlay(&close);

        let line = countdown_line();
        root.add_overlay(&line);

        let shared = Rc::new(Shared {
            notification: RefCell::new(popup.notification.clone()),
            countdown: RefCell::new(None),
            ticking: Cell::new(false),
            line,
            look: look.clone(),
            theme: theme.clone(),
        });
        {
            // The line reads the Theme at draw time, so a Theme change
            // recolours it without rebuilding the card.
            let weak = Rc::downgrade(&shared);
            shared.line.set_draw_func(move |_, cr, width, height| {
                let Some(shared) = weak.upgrade() else {
                    return;
                };
                let Some(fraction) = shared
                    .countdown
                    .borrow()
                    .map(|c| c.fraction(Instant::now()))
                else {
                    return;
                };
                draw_line(cr, width, height, fraction, &shared.theme.borrow());
            });
        }

        let hover = gtk4::EventControllerMotion::new();
        {
            let events = events.clone();
            hover.connect_enter(move |_, _, _| {
                tracing::trace!(id, "pointer entered Popup");
                send(&events, Event::HoverStarted(id));
            });
        }
        {
            let events = events.clone();
            hover.connect_leave(move |_| send(&events, Event::HoverEnded(id)));
        }
        root.add_controller(hover);

        let click = gtk4::GestureClick::new();
        click.set_button(0);
        {
            let events = events.clone();
            let shared = Rc::downgrade(&shared);
            click.connect_released(move |gesture, _, x, y| {
                let Some(shared) = shared.upgrade() else {
                    return;
                };
                if gesture.current_button() == gdk::BUTTON_PRIMARY && on_button(gesture, x, y) {
                    return;
                }
                if let Some(event) =
                    click_event(gesture.current_button(), &shared.notification.borrow())
                {
                    send(&events, event);
                }
            });
        }
        root.add_controller(click);

        let card = Self {
            id,
            root,
            shared,
            events: events.clone(),
        };
        card.fill(popup);
        card
    }

    pub fn widget(&self) -> &Overlay {
        &self.root
    }

    /// Show a replaced Notification in place; its countdown starts over.
    pub fn update(&self, popup: &Popup) {
        *self.shared.notification.borrow_mut() = popup.notification.clone();
        self.fill(popup);
    }

    /// The engine paused this Popup's countdown.
    pub fn pause_countdown(&self, remaining: Duration) {
        if let Some(countdown) = self.shared.countdown.borrow_mut().as_mut() {
            countdown.pause(remaining);
        }
        self.shared.line.queue_draw();
    }

    /// The engine resumed this Popup's countdown.
    pub fn resume_countdown(&self, remaining: Duration) {
        if let Some(countdown) = self.shared.countdown.borrow_mut().as_mut() {
            countdown.resume(remaining, Instant::now());
        }
        tick(&self.shared);
    }

    fn fill(&self, popup: &Popup) {
        let n = &popup.notification;
        let look = &self.shared.look;
        for class in ["critical", "low"] {
            self.root.remove_css_class(class);
        }
        match n.hints.urgency {
            Urgency::Critical => self.root.add_css_class("critical"),
            Urgency::Low => self.root.add_css_class("low"),
            Urgency::Normal => {}
        }

        let content = GtkBox::new(Orientation::Vertical, 0);
        content.append(&top_row(popup, look));
        if let Some(actions) = action_row(n, &self.events) {
            content.append(&actions);
        }
        self.root.set_child(Some(&content));

        let countdown = popup
            .timeout
            .filter(|_| look.countdown)
            .map(|total| Countdown::start(total, Instant::now()));
        self.shared.line.set_visible(countdown.is_some());
        *self.shared.countdown.borrow_mut() = countdown;
        tick(&self.shared);
    }
}

/// What a click on the card means: left invokes the default Action (or
/// dismisses when there is none), middle dismisses every Popup, right
/// dismisses this one.
pub fn click_event(button: u32, notification: &Notification) -> Option<Event> {
    let id = notification.id;
    match button {
        gdk::BUTTON_PRIMARY => Some(if notification.actions.iter().any(|a| a.key == "default") {
            Event::ActionInvoked {
                id,
                key: "default".into(),
            }
        } else {
            Event::UserDismissed(id)
        }),
        gdk::BUTTON_MIDDLE => Some(Event::UserDismissedAll),
        gdk::BUTTON_SECONDARY => Some(Event::UserDismissed(id)),
        _ => None,
    }
}

fn send(events: &UnboundedSender<Event>, event: Event) {
    if events.send(event).is_err() {
        tracing::warn!("engine is gone; dropping UI event");
    }
}

/// Redraw the countdown line every frame while it runs.
fn tick(shared: &Rc<Shared>) {
    shared.line.queue_draw();
    let running = shared.countdown.borrow().is_some_and(|c| c.is_running());
    if !running || shared.ticking.replace(true) {
        return;
    }
    let weak = Rc::downgrade(shared);
    shared.line.add_tick_callback(move |line, _| {
        let Some(shared) = weak.upgrade() else {
            return glib::ControlFlow::Break;
        };
        line.queue_draw();
        let running = shared
            .countdown
            .borrow()
            .is_some_and(|c| c.is_running() && c.fraction(Instant::now()) > 0.0);
        if running {
            glib::ControlFlow::Continue
        } else {
            shared.ticking.set(false);
            glib::ControlFlow::Break
        }
    });
}

fn countdown_line() -> gtk4::DrawingArea {
    let line = gtk4::DrawingArea::new();
    line.add_css_class("countdown");
    line.set_content_height(LINE_HEIGHT);
    line.set_valign(Align::End);
    line.set_hexpand(true);
    line.set_can_target(false);
    line.set_opacity(LINE_OPACITY);
    line
}

/// A gradient from accent-1 to accent-2, `fraction` of the width,
/// anchored left.
fn draw_line(cr: &gtk4::cairo::Context, width: i32, height: i32, fraction: f64, theme: &Theme) {
    let width = f64::from(width);
    let gradient = gtk4::cairo::LinearGradient::new(0.0, 0.0, width, 0.0);
    for (offset, colour) in [(0.0, theme.accent_1), (1.0, theme.accent_2)] {
        let (r, g, b, a) = colour.components();
        gradient.add_color_stop_rgba(offset, r, g, b, a);
    }
    let _ = cr.set_source(&gradient);
    cr.rectangle(
        0.0,
        0.0,
        width * fraction.clamp(0.0, 1.0),
        f64::from(height),
    );
    let _ = cr.fill();
}

/// The icon tile and the text column.
fn top_row(popup: &Popup, look: &Look) -> GtkBox {
    // The media box is 5px larger than the tile on every side, so the
    // padding and spacing are 5px smaller than the design's 14/12px.
    let overhang = look.tile_overhang();
    let top = GtkBox::new(Orientation::Horizontal, 12 - overhang);
    top.add_css_class("top");
    top.set_margin_top(14 - overhang);
    top.set_margin_start(14 - overhang);
    top.set_margin_end(14);
    top.set_margin_bottom(13 - overhang);
    top.append(&media(&popup.notification, look));
    top.append(&main_column(popup, overhang));
    top
}

/// The icon tile, with the app badge when an image takes the tile. #7
/// overlays the progress ring here.
fn media(n: &Notification, look: &Look) -> Overlay {
    let side = look.tile + 2 * look.tile_overhang();
    let media = Overlay::new();
    media.add_css_class("media");
    media.set_size_request(side, side);
    media.set_valign(Align::Start);

    let plan = icon::plan(n);
    let icon = plan.icons.iter().find_map(|s| paintable(s, look.icon_size));
    let art = plan.art.as_ref().and_then(|art| match art {
        Art::Pixels(data) => texture_from_pixels(data),
        Art::Source(source) => paintable(source, look.tile),
    });

    let tile = Tile::new(look.tile, look.tile_radius());
    tile.add_css_class("tile");
    tile.set_halign(Align::Center);
    tile.set_valign(Align::Center);
    match (&art, &icon) {
        (Some(art), _) => tile.set_paintable(art, Fit::Cover),
        (None, Some(icon)) => tile.set_paintable(icon, Fit::Contain(look.icon_size as f32)),
        (None, None) => return fallback_media(media, tile, &n.app_name),
    }
    media.set_child(Some(&tile));

    if let (Some(_), Some(icon)) = (&art, &icon) {
        let badge = Tile::new(look.badge(), 6.0);
        badge.add_css_class("badge");
        badge.set_paintable(icon, Fit::Contain(look.badge() as f32));
        badge.set_halign(Align::End);
        badge.set_valign(Align::End);
        media.add_overlay(&badge);
    }
    media
}

/// A tile in the app's own hue with its initial, for apps without an icon.
fn fallback_media(media: Overlay, tile: Tile, app_name: &str) -> Overlay {
    let hue = icon::hue(app_name);
    let rgba = |c: Rgba| {
        let (r, g, b, a) = c.components();
        gdk::RGBA::new(r as f32, g as f32, b as f32, a as f32)
    };
    tile.add_css_class("fallback");
    tile.set_gradient(
        rgba(Rgba::hsl(hue, 0.8, 0.62)),
        rgba(Rgba::hsl(hue + 30.0, 0.7, 0.46)),
    );
    media.set_child(Some(&tile));
    let letter = Label::new(Some(&icon::initial(app_name)));
    letter.add_css_class("tile");
    letter.add_css_class("fallback");
    letter.set_halign(Align::Center);
    letter.set_valign(Align::Center);
    letter.set_can_target(false);
    media.add_overlay(&letter);
    media
}

/// Load an icon or image at `size` pixels; `None` if it can't be found.
fn paintable(source: &Source, size: i32) -> Option<gdk::Paintable> {
    match source {
        Source::Themed(name) => {
            let display = gdk::Display::default()?;
            let theme = gtk4::IconTheme::for_display(&display);
            theme.has_icon(name).then(|| {
                theme
                    .lookup_icon(
                        name,
                        &[],
                        size,
                        1,
                        gtk4::TextDirection::None,
                        gtk4::IconLookupFlags::empty(),
                    )
                    .upcast()
            })
        }
        Source::File(path) => file_paintable(&gio::File::for_path(path), size),
        Source::Uri(uri) => file_paintable(&gio::File::for_uri(uri), size),
    }
}

fn file_paintable(file: &gio::File, size: i32) -> Option<gdk::Paintable> {
    if !file.query_exists(gio::Cancellable::NONE) {
        return None;
    }
    match gdk::Texture::from_file(file) {
        Ok(texture) => Some(texture.upcast()),
        // Not a raster image GDK can load (an SVG, say): let the icon
        // machinery render it at the size we need.
        Err(_) => Some(gtk4::IconPaintable::for_file(file, size, 1).upcast()),
    }
}

fn texture_from_pixels(data: &lucent::engine::ImageData) -> Option<gdk::Paintable> {
    let has_alpha = icon::pixel_layout(data)?;
    let format = if has_alpha {
        gdk::MemoryFormat::R8g8b8a8
    } else {
        gdk::MemoryFormat::R8g8b8
    };
    let bytes = glib::Bytes::from(&data.data[..]);
    let texture = gdk::MemoryTexture::new(
        data.width,
        data.height,
        format,
        &bytes,
        data.rowstride as usize,
    );
    Some(texture.upcast())
}

/// App name and time, summary, body.
fn main_column(popup: &Popup, overhang: i32) -> GtkBox {
    let n = &popup.notification;
    let main = GtkBox::new(Orientation::Vertical, 3);
    main.add_css_class("main");
    main.set_hexpand(true);
    main.set_valign(Align::Start);
    // Line the text up with the tile, not the media box around it.
    main.set_margin_top(overhang);
    main.set_margin_bottom(overhang);

    let head = GtkBox::new(Orientation::Horizontal, 6);
    head.add_css_class("head");
    // Keep clear of the close button.
    head.set_margin_end(24);
    let app = label(&n.app_name, "app");
    app.set_ellipsize(pango::EllipsizeMode::End);
    app.set_valign(Align::BaselineCenter);
    head.append(&app);
    let time = label("", "time");
    time.set_valign(Align::BaselineCenter);
    keep_time_fresh(&time, popup.received_at);
    head.append(&time);
    main.append(&head);

    if !n.summary.is_empty() {
        let summary = label(&n.summary, "summary");
        summary.set_wrap(true);
        summary.set_wrap_mode(pango::WrapMode::WordChar);
        // Ask for no natural width so the label wraps to the column.
        summary.set_max_width_chars(1);
        summary.set_hexpand(true);
        main.append(&summary);
    }

    if !n.body.is_empty() {
        let body = Label::new(None);
        body.add_css_class("body");
        body.set_markup(&body_markup(&n.body));
        body.set_xalign(0.0);
        body.set_wrap(true);
        body.set_wrap_mode(pango::WrapMode::WordChar);
        body.set_lines(3);
        body.set_ellipsize(pango::EllipsizeMode::End);
        // Ask for no natural width so the label wraps to the column.
        body.set_max_width_chars(1);
        body.set_hexpand(true);
        // Links would make the label focusable, and nothing on a Popup
        // takes the keyboard.
        body.set_can_focus(false);
        if body.text().is_empty() {
            // Markup GTK still refused: show the raw text instead.
            body.set_markup(&escape(&n.body));
        }
        main.append(&body);
    }
    main
}

fn label(text: &str, class: &str) -> Label {
    let label = Label::new(Some(text));
    label.add_css_class(class);
    label.set_xalign(0.0);
    label
}

/// Show "now"/"12s"/"3m" and update it only when the text changes. Stops
/// once the label is gone.
fn keep_time_fresh(time: &Label, received_at: Instant) {
    let elapsed = received_at.elapsed();
    time.set_text(&ago(elapsed));
    let weak = time.downgrade();
    glib::timeout_add_local_once(until_change(elapsed), move || {
        if let Some(time) = weak.upgrade()
            && time.parent().is_some()
        {
            keep_time_fresh(&time, received_at);
        }
    });
}

/// The Actions that get a button. "default" has none: a click on the
/// body invokes it.
fn buttons(notification: &Notification) -> Vec<&Action> {
    notification
        .actions
        .iter()
        .filter(|a| a.key != "default")
        .collect()
}

/// The full-width Action row: equal buttons divided by hairlines, the
/// first one primary.
fn action_row(
    notification: &Notification,
    events: &UnboundedSender<Event>,
) -> Option<gtk4::Widget> {
    let actions = buttons(notification);
    if actions.is_empty() {
        return None;
    }
    let row = GtkBox::new(Orientation::Horizontal, 0);
    row.add_css_class("actions");
    row.set_homogeneous(true);
    for (index, action) in actions.into_iter().enumerate() {
        let text = if action.label.is_empty() {
            &action.key
        } else {
            &action.label
        };
        let label = Label::new(Some(text));
        label.set_ellipsize(pango::EllipsizeMode::End);
        let button = gtk4::Button::new();
        button.set_child(Some(&label));
        button.set_tooltip_text(Some(text));
        button.set_can_focus(false);
        if index == 0 {
            button.add_css_class("primary");
        }
        let events = events.clone();
        let id = notification.id;
        let key = action.key.clone();
        button.connect_clicked(move |_| {
            send(
                &events,
                Event::ActionInvoked {
                    id,
                    key: key.clone(),
                },
            );
        });
        row.append(&button);
    }
    Some(row.upcast())
}

/// Whether a primary click landed on a button (an Action or close), which
/// handles it itself.
fn on_button(gesture: &gtk4::GestureClick, x: f64, y: f64) -> bool {
    let Some(card) = gesture.widget() else {
        return false;
    };
    let mut widget = card.pick(x, y, gtk4::PickFlags::DEFAULT);
    while let Some(w) = widget {
        if w == card {
            break;
        }
        if w.is::<gtk4::Button>() {
            return true;
        }
        widget = w.parent();
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;
    use lucent::engine::Expiry;

    fn notification(actions: &[&str]) -> Notification {
        Notification {
            id: 7,
            app_name: "chat".into(),
            app_icon: String::new(),
            summary: "hi".into(),
            body: String::new(),
            actions: actions
                .iter()
                .map(|key| Action {
                    key: (*key).into(),
                    label: (*key).into(),
                })
                .collect(),
            hints: Default::default(),
            expiry: Expiry::Default,
        }
    }

    #[test]
    fn left_click_invokes_the_default_action_when_there_is_one() {
        assert_eq!(
            click_event(1, &notification(&["reply", "default"])),
            Some(Event::ActionInvoked {
                id: 7,
                key: "default".into()
            })
        );
    }

    #[test]
    fn left_click_without_a_default_action_dismisses() {
        assert_eq!(
            click_event(1, &notification(&["reply"])),
            Some(Event::UserDismissed(7))
        );
    }

    #[test]
    fn every_action_but_default_gets_a_button_in_the_apps_order() {
        let n = notification(&["reply", "default", "read"]);
        let keys: Vec<_> = buttons(&n).iter().map(|a| a.key.as_str()).collect();
        assert_eq!(keys, ["reply", "read"]);
        assert!(buttons(&notification(&["default"])).is_empty());
    }

    #[test]
    fn right_click_dismisses_and_middle_click_dismisses_all() {
        let n = notification(&["default"]);
        assert_eq!(click_event(3, &n), Some(Event::UserDismissed(7)));
        assert_eq!(click_event(2, &n), Some(Event::UserDismissedAll));
        assert_eq!(click_event(8, &n), None);
    }
}
