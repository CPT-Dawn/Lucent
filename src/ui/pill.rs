//! The "+N more" pill: shown after the oldest Popup while Notifications
//! wait in Overflow. Clicking it dismisses the oldest Popup, which brings
//! in the next.
//!
//! ```text
//! row (full card width, so the stack places it like a card)
//! └─ button.overflow-pill (centred)
//!    ├─ .dots: three 5px gradient dots
//!    └─ label "+N more"
//! ```

use gtk4::prelude::*;
use gtk4::{Align, Box as GtkBox, Label, Orientation};
use lucent::engine::Event;
use tokio::sync::mpsc::UnboundedSender;

/// What the pill says for `count` waiting Notifications.
pub fn label(count: usize) -> String {
    format!("+{count} more")
}

/// The pill widget. It always exists; the stack shows it while Overflow
/// isn't empty.
pub struct Pill {
    row: GtkBox,
    button: gtk4::Button,
    text: Label,
}

impl Pill {
    pub fn new(width: i32, events: &UnboundedSender<Event>) -> Self {
        let dots = GtkBox::new(Orientation::Horizontal, 3);
        dots.add_css_class("dots");
        dots.set_valign(Align::Center);
        for _ in 0..3 {
            let dot = GtkBox::new(Orientation::Horizontal, 0);
            dot.add_css_class("dot");
            dots.append(&dot);
        }
        let text = Label::new(None);
        let content = GtkBox::new(Orientation::Horizontal, 8);
        content.append(&dots);
        content.append(&text);

        let button = gtk4::Button::new();
        button.add_css_class("overflow-pill");
        button.set_child(Some(&content));
        button.set_tooltip_text(Some("Show the next one (dismisses the oldest)"));
        button.set_can_focus(false);
        button.set_halign(Align::Center);
        {
            let events = events.clone();
            button.connect_clicked(move |_| {
                if events.send(Event::OverflowAdvanced).is_err() {
                    tracing::warn!("engine is gone; dropping the pill click");
                }
            });
        }

        let row = GtkBox::new(Orientation::Horizontal, 0);
        row.set_size_request(width, -1);
        row.append(&button);
        button.set_hexpand(true);
        Self { row, button, text }
    }

    /// The widget the stack places, as wide as a card.
    pub fn widget(&self) -> &GtkBox {
        &self.row
    }

    pub fn set_width(&self, width: i32) {
        self.row.set_size_request(width, -1);
    }

    /// Show how many Notifications wait. Zero keeps the old text, so the
    /// pill doesn't read "+0 more" while it fades out.
    pub fn set_count(&self, count: usize) {
        if count > 0 {
            self.text.set_label(&label(count));
        }
    }

    /// The pill's own width, centred in the row: the part that takes
    /// pointer input.
    pub fn natural_width(&self) -> i32 {
        self.button.measure(Orientation::Horizontal, -1).1
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_pill_counts_what_waits_in_overflow() {
        assert_eq!(label(3), "+3 more");
        assert_eq!(label(1), "+1 more");
    }
}
