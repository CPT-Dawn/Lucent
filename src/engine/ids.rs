//! Notification id allocation, shared between the D-Bus adapter (which must
//! answer `Notify` with an id straight away) and the engine (which needs
//! fresh ids to restore History entries).

use std::sync::Arc;
use std::sync::atomic::{AtomicU32, Ordering};

use super::notification::NotificationId;

/// Hands out increasing, non-zero Notification ids. Clones share a counter.
#[derive(Debug, Clone)]
pub struct IdAllocator(Arc<AtomicU32>);

impl IdAllocator {
    pub fn new() -> Self {
        Self::starting_at(1)
    }

    /// An allocator whose first id is `first` (0 is skipped).
    pub fn starting_at(first: NotificationId) -> Self {
        Self(Arc::new(AtomicU32::new(first)))
    }

    /// The next unused id. Skips 0 if the counter ever wraps.
    pub fn next(&self) -> NotificationId {
        loop {
            let id = self.0.fetch_add(1, Ordering::Relaxed);
            if id != 0 {
                return id;
            }
        }
    }
}

impl Default for IdAllocator {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ids_start_at_one_and_are_shared_between_clones() {
        let ids = IdAllocator::new();
        let other = ids.clone();
        assert_eq!(ids.next(), 1);
        assert_eq!(other.next(), 2);
        assert_eq!(ids.next(), 3);
    }
}
