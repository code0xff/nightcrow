//! Hub handles only the tests need.
//!
//! Beside the hub rather than inside it, so the production surface stays the
//! one the transports use. `mod.rs` declares this under `#[cfg(test)]`, so none
//! of it exists in a release build.

use super::frame::TerminalFrame;
use super::{ConcurrencyTestPoint, TerminalHub, hub_helpers};
use crate::backend::PaneId;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

/// What a test can observe about the worker without reaching into its loop.
#[derive(Default)]
pub(super) struct WorkerProbe {
    /// Passes the worker has started, so a test can tell a parked worker from a
    /// busy one and measure how often an idle hub wakes.
    pub(super) passes: AtomicU64,
    /// Set when the worker returns, whichever way it ended.
    pub(super) done: AtomicBool,
    /// Reproduce the fixed tick the worker used before it could be woken, for
    /// the measurement that compares the two.
    pub(super) legacy_polling: AtomicBool,
    /// Replaces the idle wait cap. `None` keeps the production one, so a test can
    /// make the computed deadline the only thing that can end a wait.
    pub(super) wait_cap: Mutex<Option<Duration>>,
    /// The deadline the worker computed at the end of its last pass. A test that
    /// means to prove a signal woke the worker also requires this to be `None`,
    /// so a pending pace or drain deadline cannot pass for one.
    pub(super) deadline: Mutex<Option<Instant>>,
}

impl TerminalHub {
    /// Passes the worker has started. A test waits for this to stop moving to
    /// know the worker is parked on its signal rather than between passes.
    pub(crate) fn worker_passes(&self) -> u64 {
        self.probe.passes.load(Ordering::Acquire)
    }

    /// Whether the worker has returned. `stop` detaches a worker it cannot join in
    /// time, so this is what says the teardown actually finished.
    pub(crate) fn worker_finished(&self) -> bool {
        self.probe.done.load(Ordering::Acquire)
    }

    /// Reproduce the pre-wakeup fixed tick, for the measurement that compares how
    /// often an idle hub wakes with and without it.
    pub(super) fn set_legacy_polling(&self, on: bool) {
        self.probe.legacy_polling.store(on, Ordering::Release);
    }

    /// Replace the idle wait cap, so a test can leave a missing signal with no
    /// rescue but its own deadline.
    pub(super) fn set_wait_cap(&self, cap: Duration) {
        *self
            .probe
            .wait_cap
            .lock()
            .expect("terminal wait cap poisoned") = Some(cap);
    }

    /// The cap the worker's next wait will use, or `None` for production's.
    pub(super) fn wait_cap(&self) -> Option<Duration> {
        *self
            .probe
            .wait_cap
            .lock()
            .expect("terminal wait cap poisoned")
    }

    /// The deadline the worker computed at the end of its last pass.
    pub(crate) fn worker_deadline(&self) -> Option<Instant> {
        *self
            .probe
            .deadline
            .lock()
            .expect("terminal deadline poisoned")
    }

    pub(super) fn record_worker_deadline(&self, deadline: Option<Instant>) {
        *self
            .probe
            .deadline
            .lock()
            .expect("terminal deadline poisoned") = deadline;
    }

    /// The stored screen and the bytes owed on top of it, for a live pane. What
    /// the trailing snapshot clears is the owed tail.
    pub(crate) fn pane_record(&self, pane: PaneId) -> Option<(usize, usize)> {
        self.state
            .lock()
            .expect("terminal state poisoned")
            .panes
            .iter()
            .find(|p| p.id == pane)
            .map(|p| (p.screen.len(), p.since.len()))
    }

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
