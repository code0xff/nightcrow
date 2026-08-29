use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

/// Bounded admission for sockets that have not identified themselves yet.
/// It is separate from attached clients because status and stop are one-shot
/// requests and must never appear in the attached-client count.
pub(super) struct PreAttachAdmission {
    active: AtomicUsize,
    limit: usize,
}

impl PreAttachAdmission {
    pub(super) fn new(limit: usize) -> Self {
        assert!(limit > 0, "pre-attach admission limit must be positive");
        Self {
            active: AtomicUsize::new(0),
            limit,
        }
    }

    pub(super) fn try_reserve(self: &Arc<Self>) -> Option<Permit> {
        let mut active = self.active.load(Ordering::Acquire);
        loop {
            if active >= self.limit {
                return None;
            }
            match self.active.compare_exchange_weak(
                active,
                active + 1,
                Ordering::AcqRel,
                Ordering::Acquire,
            ) {
                Ok(_) => return Some(Permit(Arc::clone(self))),
                Err(observed) => active = observed,
            }
        }
    }

    #[cfg(test)]
    pub(super) fn active(&self) -> usize {
        self.active.load(Ordering::Acquire)
    }
}

/// A pre-attach slot that releases itself on every return, including errors
/// and thread-spawn failures.
pub(super) struct Permit(Arc<PreAttachAdmission>);

impl Drop for Permit {
    fn drop(&mut self) {
        self.0.active.fetch_sub(1, Ordering::AcqRel);
    }
}

#[cfg(test)]
#[path = "admission_tests.rs"]
mod tests;
