//! Both halves of a backend's wake contract with its owner: how the PTY threads
//! tell it that an event is queued, and when the backend's own timed state wants
//! it back.

use super::{EXIT_DRAIN_GRACE, ExitPhase, PtyBackend};
use crate::platform::threading::WakeHandle;
use std::time::Instant;

impl PtyBackend {
    /// Hand this backend the owner's wake signal, so its reader and waiter
    /// threads report queued events instead of relying on a poll.
    pub fn set_waker(&mut self, waker: WakeHandle) {
        self.waker = Some(waker);
    }

    /// The earliest moment a drain has timed work of its own: a child that ended
    /// while its pane's last output was still being drained.
    ///
    /// Only [`ExitPhase::Draining`] has a clock. Every other thing a drain
    /// reports arrives from a thread that signals the owner itself, so the
    /// deadline is empty when nothing is waiting out the grace.
    pub fn next_deadline(&self) -> Option<Instant> {
        self.panes
            .values()
            .filter_map(|pane| match pane.exit {
                ExitPhase::Draining { since } => Some(since + EXIT_DRAIN_GRACE),
                ExitPhase::Running | ExitPhase::Reported => None,
            })
            .min()
    }
}
