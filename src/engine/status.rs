//! Live status: the History count and Do Not Disturb state.
//!
//! Whenever either changes, the feature that changed it pushes
//! `Effect::StatusChanged(self.status())` (History lands in ticket #9, Do
//! Not Disturb in ticket #10).

use super::{Engine, Status};

impl Engine {
    /// The current status, as `lucent status` reports it.
    pub fn status(&self) -> Status {
        Status::default()
    }
}
