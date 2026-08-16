//! Ending a synchronized update (DEC 2026) the program never closed.
//!
//! Between `BSU` and `ESU` the processor holds the update's bytes back from the
//! grid, and ends the update only on `ESU` or at its own 2 MiB buffer cap. A
//! program that dies mid-frame — a TUI killed on exit, or one re-execing itself
//! to update — sends neither, and the pane it leaves behind produces nothing
//! but a shell prompt afterwards, so the cap is never reached either: the grid
//! stops moving for good while the shell underneath still takes input. The
//! pane looks frozen and is not.
//!
//! vte's answer is the 150 ms timeout it arms on `BSU` and leaves for its
//! caller to honour (alacritty ticks it from its event loop). Every owner of a
//! `PaneEmulator` ticks it here.

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
    /// [`sync_expired`](Self::sync_expired) so a live update is never cut short.
    pub fn settle_sync(&mut self) -> EmulatorEvents {
        self.processor.stop_sync(&mut self.term);
        self.take_events()
    }
}
