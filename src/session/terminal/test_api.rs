//! Hub handles only the tests need.
//!
//! Beside the hub rather than inside it, so the production surface stays the
//! one the transports use. `mod.rs` declares this under `#[cfg(test)]`, so none
//! of it exists in a release build.

use super::frame::TerminalFrame;
use super::{ConcurrencyTestPoint, TerminalHub, hub_helpers};
use std::sync::Arc;

impl TerminalHub {
    pub fn client_count(&self) -> usize {
        self.state
            .lock()
            .expect("terminal state poisoned")
            .clients
            .len()
    }

    /// Queue a frame for every client, as a broadcast would.
    ///
    /// The web connection tests need the hub to stream without a shell producing
    /// output, so that what the writer does under backpressure can be observed
    /// deterministically.
    pub(crate) fn test_broadcast(&self, frame: TerminalFrame) {
        let mut state = self.state.lock().expect("terminal state poisoned");
        hub_helpers::broadcast_locked(&mut state.clients, frame);
    }

    pub(super) fn set_concurrency_test_hook(
        &self,
        hook: impl Fn(ConcurrencyTestPoint) + Send + Sync + 'static,
    ) {
        *self
            .concurrency_test_hook
            .lock()
            .expect("terminal concurrency test hook poisoned") = Some(Arc::new(hook));
    }

    pub(super) fn run_concurrency_test_hook(&self, point: ConcurrencyTestPoint) {
        let hook = self
            .concurrency_test_hook
            .lock()
            .expect("terminal concurrency test hook poisoned")
            .clone();
        if let Some(hook) = hook {
            hook(point);
        }
    }
}
