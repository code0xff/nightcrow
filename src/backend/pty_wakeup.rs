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
    /// while its pane's last output was still being drained, or a pane whose drain
    /// spent its per-pane budget with events still queued.
    ///
    /// The backlog is due at once rather than at a clock: the reader signalled when
    /// it queued those events, and that signal was already consumed by the pass
    /// that spent the budget. Returning `now` makes the owner's next wait return
    /// immediately so the following pass takes more; every such pass drains at
    /// least one event, so this cannot spin. A drain that saw its pane dry records
    /// no backlog, and every other thing a drain reports arrives from a thread that
    /// signals the owner itself.
    pub fn next_deadline(&self) -> Option<Instant> {
        if self.drain_backlog {
            return Some(Instant::now());
        }
        self.panes
            .values()
            .filter_map(|pane| match pane.exit {
                ExitPhase::Draining { since } => Some(since + EXIT_DRAIN_GRACE),
                ExitPhase::Running | ExitPhase::Reported => None,
            })
            .min()
    }
}
