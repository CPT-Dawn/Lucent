//! GTK adapter: draws the Popups the engine asks for and turns pointer
//! input into engine events. All state lives on the GTK main thread.
//!
//! | module      | owns                                                  |
//! |-------------|-------------------------------------------------------|
//! | `stack`     | the layer-shell window and enter/leave/reflow motion  |
//! | `card`      | one Popup card and its pointer input                  |
//! | `widgets`   | the icon `Tile` and the transform-placed `StackView`  |
//! | `theme`     | Theme colour tokens, the Look, the generated CSS      |
//! | `style`     | the display-wide stylesheet, reloadable               |
//! | `layout`    | stack geometry (pure)                                 |
//! | `motion`    | easing curves, durations, tweens (pure)               |
//! | `icon`      | what goes in the icon tile (pure)                     |
//! | `markup`    | body markup checking (pure)                           |
//! | `countdown` | the countdown line's state (pure)                     |
//! | `progress`  | the progress ring and header percentage (pure)       |
//! | `ago`       | the relative time text (pure)                         |

pub mod ago;
pub mod card;
pub mod countdown;
pub mod icon;
pub mod layout;
pub mod markup;
pub mod motion;
pub mod progress;
pub mod stack;
pub mod style;
pub mod theme;
pub mod widgets;

use std::cell::RefCell;
use std::rc::Rc;

use lucent::config::Config;
use lucent::engine::{Effect, Event, Popup};
use tokio::sync::mpsc::UnboundedSender;

use card::Card;
use stack::Stack;
use theme::{Look, Theme};

/// The Popups on screen.
pub struct Ui {
    look: RefCell<Look>,
    /// Shared with every card, which reads it when drawing.
    theme: Rc<RefCell<Theme>>,
    events: UnboundedSender<Event>,
    stack: Stack,
}

impl Ui {
    pub fn new(config: Rc<Config>, events: UnboundedSender<Event>) -> Self {
        let look = Look::from_config(&config);
        let theme = Theme::dawn();
        Self {
            stack: Stack::new(&look, &theme),
            look: RefCell::new(look),
            theme: Rc::new(RefCell::new(theme)),
            events,
        }
    }

    /// Use a reloaded config: placement and width apply at once, the rest
    /// to Popups shown or replaced from now on.
    pub fn replace_config(&self, config: Rc<Config>) {
        let look = Look::from_config(&config);
        self.stack.set_look(&look, &self.theme.borrow());
        *self.look.borrow_mut() = look;
    }

    /// Apply one engine effect. Effects that aren't about Popups are
    /// ignored.
    pub fn apply(&self, effect: &Effect) {
        match effect {
            Effect::ShowPopup(popup) => self.show(popup),
            Effect::UpdatePopup(popup) => self.stack.update(popup),
            Effect::RemovePopup { id, reason } => {
                tracing::debug!(id, reason = reason.code(), "removing Popup");
                self.stack.remove(*id);
            }
            Effect::CountdownPaused { id, remaining } => {
                tracing::debug!(id, ?remaining, "countdown paused");
                self.stack
                    .with_card(*id, |card| card.pause_countdown(*remaining));
            }
            Effect::CountdownResumed { id, remaining } => {
                tracing::debug!(id, ?remaining, "countdown resumed");
                self.stack
                    .with_card(*id, |card| card.resume_countdown(*remaining));
            }
            _ => {}
        }
    }

    fn show(&self, popup: &Popup) {
        let n = &popup.notification;
        tracing::info!(id = n.id, app = %n.app_name, summary = %n.summary, "showing Popup");
        let card = Card::new(popup, &self.look.borrow(), &self.theme, &self.events);
        self.stack.show(card);
    }
}
