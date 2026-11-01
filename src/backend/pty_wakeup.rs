//! Both halves of a backend's wake contract with its owner: how the PTY threads
//! tell it that an event is queued, and when the backend's own timed state wants
//! it back.

use super::{EXIT_DRAIN_GRACE, ExitPhase, PER_PANE_DRAIN_BUDGET, PtyBackend};
use crate::platform::threading::WakeHandle;
use std::time::Instant;

impl PtyBackend {
    /// Hand this backend the owner's wake signal, so its reader and waiter
    /// threads report queued events instead of relying on a poll.
    pub fn set_waker(&mut self, waker: WakeHandle) {
        self.waker = Some(waker);
    }

    /// The earliest moment a drain has timed work of its own: a child that ended
    /// while its pane's last output was still being drained, or a pane whose drain
    /// spent its window's budget and waits for the window to end.
    ///
    /// The backlog needs a deadline because the reader signalled when it queued
    /// those events, and that signal was already consumed by an earlier pass. A
    /// pass at the window's end opens a new window for that pane, so this cannot
    /// spin. A pane with budget left needs no deadline, and every other thing a
    /// drain reports arrives from a thread that signals the owner itself.
    pub fn next_deadline(&self) -> Option<Instant> {
        self.panes
            .values()
            .flat_map(|pane| {
                let exit = match pane.exit {
                    ExitPhase::Draining { since } => Some(since + EXIT_DRAIN_GRACE),
                    ExitPhase::Running | ExitPhase::Reported => None,
                };
                let spent = pane
                    .window
                    .filter(|window| window.used >= PER_PANE_DRAIN_BUDGET)
                    .map(|window| window.end());
                exit.into_iter().chain(spent)
            })
            .min()
    }
}
