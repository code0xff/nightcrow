//! Ending a synchronized update (DEC 2026) the program never closed.
//!
//! Between `BSU` and `ESU` the processor holds the update's bytes back from
//! the grid and ends it only on `ESU` or at its own 2 MiB buffer cap — a
//! program killed mid-frame sends neither, so the pane looks frozen while the
//! shell underneath still takes input. vte arms the 150 ms timeout on `BSU`
//! and leaves it for its caller to honour; every owner of a `PaneEmulator`
//! ticks it here.

use super::{EmulatorEvents, PaneEmulator};
use std::time::Instant;

impl PaneEmulator {
    /// Whether an open synchronized update has outlived its timeout as of
    /// `now`. False when no update is open.
    pub fn sync_expired(&self, now: Instant) -> bool {
        self.processor
            .sync_timeout()
            .sync_timeout()
            .is_some_and(|deadline| deadline <= now)
    }

    /// End an open synchronized update, applying to the grid the bytes it held
    /// back. Harmless when none is open, but callers gate on
    /// [`sync_expired`](Self::sync_expired) so a live update is never cut
    /// short.
    pub fn settle_sync(&mut self) -> EmulatorEvents {
        self.processor.stop_sync(&mut self.term);
        self.take_events()
    }
}
