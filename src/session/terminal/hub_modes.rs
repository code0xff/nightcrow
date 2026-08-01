//! What state each pane's program has put its terminal into.
//!
//! A client that attaches to a pane mid-session is replayed a window of the
//! pane's output, and the bytes that set the pane's modes are almost never in it
//! — a program announces them once, at startup, and the ring has long since
//! evicted that. So the hub follows them here and hands a connecting client the
//! answer directly (see `PaneModes::prelude`).
//!
//! Kept on the worker thread rather than in [`Shared`](super::Shared): a
//! `PaneEmulator` holds `Rc`, so it is not `Send` and cannot live behind the
//! state mutex. What crosses the lock is the plain flag set the worker writes
//! into `PaneState` after each chunk.
//!
//! **The grid is not consulted.** An emulator needs one to parse into, and this
//! one is opened at the pane's size and then left alone — resizes are not
//! followed, because nothing here reads a cell. Anything that starts reading the
//! screen has to start following them.

use crate::backend::PaneId;
use crate::runtime::emulator::{PaneEmulator, PaneModes};
use std::collections::HashMap;

/// Scrollback for the tracking emulators: none. Their grids exist so the parser
/// has somewhere to put what it parses, and history would be paid for per pane
/// on top of the ring the hub already keeps.
const NO_HISTORY: usize = 0;

#[derive(Default)]
pub(super) struct PaneModeTracker {
    emulators: HashMap<PaneId, PaneEmulator>,
}

impl PaneModeTracker {
    /// Feed a pane's output through its emulator and report the modes it is now
    /// in.
    ///
    /// The emulator is opened on the pane's first chunk, which is why `size`
    /// arrives as a closure: it reads the shared state, and the caller must not
    /// already hold that lock.
    pub(super) fn observe(
        &mut self,
        pane: PaneId,
        data: &[u8],
        size: impl FnOnce() -> (u16, u16),
    ) -> PaneModes {
        let emulator = self.emulators.entry(pane).or_insert_with(|| {
            let (rows, cols) = size();
            PaneEmulator::new(rows, cols, NO_HISTORY)
        });
        emulator.process(data);
        emulator.modes()
    }

    /// Forget a pane whose process is gone. A relaunch comes back under a new
    /// pane id and opens a fresh emulator on its first chunk, which is correct:
    /// the modes belong to the program, not to the slot.
    pub(super) fn forget(&mut self, pane: PaneId) {
        self.emulators.remove(&pane);
    }
}
