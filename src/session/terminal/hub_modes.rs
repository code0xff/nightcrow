//! What state each pane's program has put its terminal into, and what it calls
//! itself, followed here because a client attaching mid-session is replayed a
//! window of output that almost never contains the bytes that set them — a
//! program announces modes once, at startup, and the ring has long since
//! evicted that. Titles have the same shape, so they are followed here too.
//!
//! Kept on the worker thread rather than in [`Shared`](super::Shared): a
//! `PaneEmulator` holds `Rc`, so it is not `Send` and cannot live behind the
//! state mutex. The grid is read (for `snapshot`), so resizes have to be
//! followed — `hub_layout::resize_pane` is the one place that must call
//! `resize`.

use crate::backend::PaneId;
use crate::runtime::emulator::{PaneEmulator, PaneModes};
use std::collections::HashMap;
use std::time::Instant;

/// Scrollback for the tracking emulators: none. The byte ring in `PaneState`
/// carries history; paying for it twice per pane would buy nothing.
const NO_HISTORY: usize = 0;

/// What one chunk of a pane's output said about it.
pub(super) struct Observed {
    pub(super) modes: PaneModes,
    /// The title this chunk set, if it set one. `None` means "unchanged", never
    /// "cleared": the emulator already drops empty and whitespace-only titles,
    /// which programs emit to mean leave it alone.
    pub(super) title: Option<String>,
    /// Whether this chunk moved the pane onto or off the alternate screen. The
    /// two sides are recorded differently — a screen against a byte ring — so the
    /// worker has to know the moment it changes rather than discover it later.
    pub(super) alt_changed: bool,
}

#[derive(Default)]
pub(super) struct PaneModeTracker {
    emulators: HashMap<PaneId, PaneEmulator>,
    /// Whether each pane was on the alternate screen as of its last chunk, so a
    /// change can be reported without the caller having to remember.
    alt_screen: HashMap<PaneId, bool>,
}

impl PaneModeTracker {
    /// Feed a pane's output through its emulator and report what it says about
    /// the pane now.
    ///
    /// The emulator is opened on the pane's first chunk, which is why `size`
    /// arrives as a closure: it reads the shared state, and the caller must not
    /// already hold that lock.
    pub(super) fn observe(
        &mut self,
        pane: PaneId,
        data: &[u8],
        size: impl FnOnce() -> (u16, u16),
    ) -> Observed {
        let emulator = self.emulators.entry(pane).or_insert_with(|| {
            let (rows, cols) = size();
            PaneEmulator::new(rows, cols, NO_HISTORY)
        });
        let events = emulator.process(data);
        let modes = emulator.modes();
        self.observed(pane, modes, events.title)
    }

    /// End every pane's synchronized update (DEC 2026) that has outlived its
    /// timeout as of `now`, and report what closing it said about those panes.
    ///
    /// A program killed mid-update never closes it, and these emulators see
    /// nothing but what their pane produces — so without a clock a pane's
    /// modes, its title, and the grid every snapshot is read from would all
    /// stay at the moment the update opened. See `runtime::emulator::sync`.
    pub(super) fn settle_sync(&mut self, now: Instant) -> Vec<(PaneId, Observed)> {
        let expired: Vec<PaneId> = self
            .emulators
            .iter()
            .filter(|(_, emulator)| emulator.sync_expired(now))
            .map(|(pane, _)| *pane)
            .collect();
        expired
            .into_iter()
            .filter_map(|pane| {
                let emulator = self.emulators.get_mut(&pane)?;
                let events = emulator.settle_sync();
                let modes = emulator.modes();
                Some((pane, self.observed(pane, modes, events.title)))
            })
            .collect()
    }

    /// What a pane's emulator says about it now, against what it said last.
    fn observed(&mut self, pane: PaneId, modes: PaneModes, title: Option<String>) -> Observed {
        // A pane nothing has been observed for counts as being on the normal
        // screen, which is where every program starts.
        let was_alt = self
            .alt_screen
            .insert(pane, modes.alt_screen)
            .unwrap_or(false);
        Observed {
            modes,
            alt_changed: was_alt != modes.alt_screen,
            // Bounded here rather than where it is shown: this one goes into
            // every connecting client's greeting, and the child process chooses
            // it.
            title: title.map(|title| {
                title
                    .chars()
                    .take(crate::session::limits::MAX_PANE_TITLE_CHARS)
                    .collect()
            }),
        }
    }

    /// Give a pane's emulator the size its PTY was just set to, so the grid the
    /// snapshot is read from wraps where the child does.
    ///
    /// A pane that has produced nothing has no emulator yet; it opens at the
    /// current size on its first chunk, so there is nothing to correct here.
    pub(super) fn resize(&mut self, pane: PaneId, rows: u16, cols: u16) {
        if let Some(emulator) = self.emulators.get_mut(&pane) {
            emulator.resize(rows, cols);
        }
    }

    /// The bytes that reproduce this pane's screen, or `None` for a pane that has
    /// produced no output yet — there is no emulator, and nothing to show.
    pub(super) fn snapshot(&self, pane: PaneId) -> Option<Vec<u8>> {
        self.emulators.get(&pane).map(|e| e.screen_snapshot())
    }

    /// Whether this pane's output so far ends with every sequence closed — the
    /// gate on anchoring a snapshot into its records. A snapshot spliced in
    /// mid-sequence would hand a reattaching client the sequence's tail as
    /// ordinary input; the caller defers to the next chunk that ends clean
    /// instead (see
    /// [`PaneEmulator::at_boundary`](crate::runtime::emulator::PaneEmulator::at_boundary)).
    /// A pane with no output yet is trivially at one.
    pub(super) fn at_boundary(&self, pane: PaneId) -> bool {
        self.emulators.get(&pane).is_none_or(|e| e.at_boundary())
    }

    /// The other half of [`at_boundary`](Self::at_boundary) on its own: whether
    /// this pane's grid holds everything recorded. The worker's desperation
    /// rule may force a snapshot over a torn sequence, but never over a grid
    /// with bytes missing — a seam is garbage on the screen once, a missing
    /// update is a screen that is simply wrong.
    pub(super) fn screen_current(&self, pane: PaneId) -> bool {
        self.emulators.get(&pane).is_none_or(|e| e.screen_current())
    }

    /// Forget a pane whose process is gone. A relaunch comes back under a new
    /// pane id and opens a fresh emulator on its first chunk, which is correct:
    /// the modes belong to the program, not to the slot.
    pub(super) fn forget(&mut self, pane: PaneId) {
        self.emulators.remove(&pane);
        self.alt_screen.remove(&pane);
    }
}
