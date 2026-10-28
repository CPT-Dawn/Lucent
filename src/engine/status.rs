//! Live status: the History count and Do Not Disturb state.
//!
//! Whenever either changes, the feature that changed it (History or Do Not
//! Disturb) pushes `Effect::StatusChanged(self.status())`.

use super::{Engine, Status};

impl Engine {
    /// The current status, as `lucent status` reports it.
    pub fn status(&self) -> Status {
        Status {
            history_count: self.history.len(),
            dnd: self.dnd.is_on(),
        }
    }
}
