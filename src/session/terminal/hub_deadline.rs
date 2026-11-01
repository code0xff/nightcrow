//! When the worker has to come back on its own.
//!
//! Between passes the worker sleeps on the hub's wake signal, and a pass ends by
//! asking every owner of timed state for the earliest moment it has work. Nothing
//! here decides *what* is due: each type answers for the clock it keeps, and the
//! pass that runs after the wait does the work, exactly as a pass after a poll did.
//!
//! **The contract a deadline carries.** The state that produced it must be cleared
//! by the pass that observes it, or the next wait returns immediately and the
//! worker spins. So a query that cannot promise that — a record whose snapshot
//! waits for a clean chunk rather than for a clock — has to answer `None` and let
//! the next producer wake decide.

use super::TerminalHub;
use super::hub_modes::PaneModeTracker;
use super::hub_plugins::Plugins;
use crate::backend::PtyBackend;
use std::time::Instant;

impl TerminalHub {
    /// The earliest moment any of the worker's timed state comes due.
    ///
    /// `None` means nothing is waiting on a clock: the next pass is owed only to a
    /// producer's signal, or to the safety cap that bounds a signal never sent.
    pub(super) fn next_deadline(
        &self,
        backend: &PtyBackend,
        modes: &PaneModeTracker,
        plugins: &Plugins,
        now: Instant,
    ) -> Option<Instant> {
        [
            backend.next_deadline(),
            modes.next_deadline(),
            plugins.next_deadline(backend, now),
            self.ownership.next_deadline(),
        ]
        .into_iter()
        .flatten()
        .min()
    }
}
