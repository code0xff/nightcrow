//! Getting a program to draw its whole screen again for a client that just
//! attached.
//!
//! A pane's recorded history is a byte window, and a program on the alternate
//! screen paints only the cells that changed — so replaying the window to a new
//! client cannot rebuild what is on screen. The bytes that could are the ones the
//! program would write if it drew again, so that is what is asked for.
//!
//! **The size has to change and change back, with a gap.** A terminal signals
//! `SIGWINCH` only when the size actually changes, so re-applying the size the
//! PTY already has reaches nobody. Two `resize` calls back to back do not work
//! either: both complete before the child's handler runs, so it reads the final
//! size, sees no change, and a program that repaints on a *changed* size does
//! nothing. So the pane is made one row shorter, and put back a tick later, once
//! the program has had time to see it.
//!
//! Clients are not told about the intermediate size — it exists for a tenth of a
//! second, the recorded size never changes, and the repaint that follows the
//! restore covers every row.

use super::TerminalHub;
use crate::backend::{PaneId, PtyBackend, TerminalBackend};
use crate::web::viewer::limits;
use std::collections::HashMap;
use std::time::{Duration, Instant};

/// How long the shorter size stays in effect. Long enough that the child's
/// `SIGWINCH` handler has run and read it, short enough to be over before
/// anyone has read a line of the screen.
const RESTORE_AFTER: Duration = Duration::from_millis(100);

/// Least time between repaints of one pane. A phone that wakes to a dead socket
/// reconnects on a one-second timer, and every attempt takes the sizing and asks
/// for a repaint; without this, a pane whose client cannot stay connected would
/// spend that whole time redrawing.
const MIN_INTERVAL: Duration = Duration::from_secs(2);

/// A pane owed its size back, and when it is due.
struct PendingRestore {
    pane: PaneId,
    due: Instant,
}

/// Worker-local bookkeeping for repaints in flight.
#[derive(Default)]
pub(super) struct Repaints {
    pending: Vec<PendingRestore>,
    last: HashMap<PaneId, Instant>,
}

impl Repaints {
    /// Whether there is nothing to do, so the worker can skip the clock read.
    pub(super) fn is_idle(&self) -> bool {
        self.pending.is_empty()
    }

    fn too_soon(&self, pane: PaneId, now: Instant) -> bool {
        self.last
            .get(&pane)
            .is_some_and(|last| now.duration_since(*last) < MIN_INTERVAL)
    }

    fn already_shrunk(&self, pane: PaneId) -> bool {
        self.pending.iter().any(|p| p.pane == pane)
    }

    fn forget(&mut self, pane: PaneId) {
        self.pending.retain(|p| p.pane != pane);
        self.last.remove(&pane);
    }
}

impl TerminalHub {
    /// Shrink each pane by a row so its program will be told the size changed.
    /// The restore is left to [`TerminalHub::finish_repaints`].
    pub(super) fn start_repaints(
        &self,
        backend: &mut PtyBackend,
        repaints: &mut Repaints,
        panes: &[PaneId],
        now: Instant,
    ) {
        for &pane in panes {
            if repaints.too_soon(pane, now) || repaints.already_shrunk(pane) {
                continue;
            }
            let Some((rows, cols)) = self.pane_size(pane) else {
                // Gone between the client attaching and this reaching the
                // worker, which is ordinary.
                continue;
            };
            backend.resize(pane, shrunk(rows), cols);
            repaints.last.insert(pane, now);
            repaints.pending.push(PendingRestore {
                pane,
                due: now + RESTORE_AFTER,
            });
        }
    }

    /// Give back the size of every pane whose gap has elapsed. The size
    /// restored is whatever the pane's record says *now*, not what it was when
    /// the repaint started — a client that resized the pane in between has the
    /// last word.
    pub(super) fn finish_repaints(
        &self,
        backend: &mut PtyBackend,
        repaints: &mut Repaints,
        now: Instant,
    ) {
        let due: Vec<PaneId> = repaints
            .pending
            .iter()
            .filter(|p| p.due <= now)
            .map(|p| p.pane)
            .collect();
        repaints.pending.retain(|p| p.due > now);
        for pane in due {
            let Some((rows, cols)) = self.pane_size(pane) else {
                repaints.forget(pane);
                continue;
            };
            backend.resize(pane, rows, cols);
        }
    }

    /// Drop a gone pane's repaint bookkeeping.
    pub(super) fn forget_repaints(&self, repaints: &mut Repaints, pane: PaneId) {
        repaints.forget(pane);
    }
}

/// One row shorter, or one taller when the pane is already at the floor — the
/// direction does not matter, only that the number is different and legal.
fn shrunk(rows: u16) -> u16 {
    if rows > limits::MIN_PANE_DIMENSION {
        rows - 1
    } else {
        rows + 1
    }
}
