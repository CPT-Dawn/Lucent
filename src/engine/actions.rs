//! Actions and click semantics.

use super::{Effect, Engine, NotificationId};

impl Engine {
    /// The user invoked an Action on a Popup.
    ///
    /// Not handled yet: ticket #6 (Actions and click semantics) emits
    /// `ActionInvoked` and closes the Popup as dismissed.
    pub(super) fn invoke_action(
        &mut self,
        _id: NotificationId,
        _key: String,
        _out: &mut Vec<Effect>,
    ) {
    }
}
