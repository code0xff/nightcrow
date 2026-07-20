//! Connection accounting for the accept loop.
//!
//! Each live connection costs a handler thread, so a server that accepts
//! without a bound lets anything reaching the port exhaust the process. The
//! cap itself is each server's policy; this module only hands out and reclaims
//! the slots.

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

/// A claimed connection slot. Releasing it is `Drop`, so every handler exit
/// path — normal return, early error, a panicking thread — frees the slot.
pub struct ConnectionSlot {
    counter: Arc<AtomicUsize>,
}

impl ConnectionSlot {
    /// Claim a slot, or return `None` when `counter` is already at `cap`.
    pub fn acquire(counter: &Arc<AtomicUsize>, cap: usize) -> Option<Self> {
        // Claim first and give back on overflow, so two accepts racing at the
        // limit cannot both see room and both proceed.
        let previous = counter.fetch_add(1, Ordering::AcqRel);
        if previous >= cap {
            counter.fetch_sub(1, Ordering::AcqRel);
            return None;
        }
        Some(Self {
            counter: Arc::clone(counter),
        })
    }
}

impl Drop for ConnectionSlot {
    fn drop(&mut self) {
        self.counter.fetch_sub(1, Ordering::AcqRel);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn connection_slot_refuses_over_the_cap() {
        let counter = Arc::new(AtomicUsize::new(0));

        let held: Vec<_> = (0..2)
            .map(|_| ConnectionSlot::acquire(&counter, 2).expect("under the cap"))
            .collect();

        assert!(
            ConnectionSlot::acquire(&counter, 2).is_none(),
            "a third connection must be refused"
        );
        assert_eq!(
            counter.load(Ordering::Acquire),
            2,
            "a refused connection must not leak a slot"
        );
        drop(held);
    }

    #[test]
    fn connection_slot_releases_on_drop() {
        let counter = Arc::new(AtomicUsize::new(0));

        drop(ConnectionSlot::acquire(&counter, 1).expect("under the cap"));

        assert_eq!(counter.load(Ordering::Acquire), 0);
        assert!(
            ConnectionSlot::acquire(&counter, 1).is_some(),
            "the freed slot must be reusable"
        );
    }
}
