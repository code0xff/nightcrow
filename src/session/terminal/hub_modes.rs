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
use std::time::{Duration, Instant};

/// Scrollback for the tracking emulators: none. The byte ring in `PaneState`
/// carries history; paying for it twice per pane would buy nothing.
const NO_HISTORY: usize = 0;

/// Least time between two end-of-tick snapshots of the same alternate-screen
/// pane. A client connecting between two of them is replayed the older screen
/// plus the bytes recorded since. Replay is exact for the state that screen
/// carries; the state it does not — scroll region, saved cursor, origin mode,
/// charset, insert mode, and autowrap as of the snapshot — stays exposed in the
/// tail for at most one interval after the pane's last write. The paced pass takes a
/// trailing snapshot once a pane that still owes bytes falls idle, so a quiet
/// pane does not keep that state exposed indefinitely.
const SNAPSHOT_INTERVAL: Duration = Duration::from_millis(250);

/// How much output waiting on top of a pane's last screen makes that wait
/// pointless. At this point the replay is already as long as the pace sets out
/// to bound, so a snapshot is taken as soon as the stream ends clean.
const SNAPSHOT_OWED_BYTES: usize = 64 * 1024;

/// How recently a pane's screen was serialized, and how much output has been
/// recorded on top of it since.
#[derive(Default)]
struct Pace {
    /// `None` until this tracker has taken one; a pane it has never
    /// snapshotted is due at the first opportunity.
    at: Option<Instant>,
    owed: usize,
}

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
    /// How often each pane's screen has been worth serializing.
    pace: HashMap<PaneId, Pace>,
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
                let observed = self.observed(pane, modes, events.title);
                if observed.alt_changed {
                    self.reset_pace(pane);
                }
                Some((pane, observed))
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
        // The screen stored before the resize describes the old grid. Drop the
        // window so the next clean boundary takes a fresh one instead of
        // waiting out a period measured against the record it replaced.
        if let Some(pace) = self.pace.get_mut(&pane) {
            pace.at = None;
        }
    }

    /// The bytes that reproduce this pane's screen, or `None` for a pane that has
    /// produced no output yet — there is no emulator, and nothing to show.
    pub(super) fn snapshot(&self, pane: PaneId) -> Option<Vec<u8>> {
        self.emulators.get(&pane).map(|e| e.screen_snapshot())
    }

    /// Note how much output a pane's record now holds on top of the screen the
    /// worker last stored — the bytes a connecting client is replayed after it.
    pub(super) fn note_owed(&mut self, pane: PaneId, owed: usize) {
        self.pace.entry(pane).or_default().owed = owed;
    }

    /// Note that `pane`'s screen was just stored, so nothing is owed on top of
    /// it and the next one waits out a full [`SNAPSHOT_INTERVAL`] from `now`.
    ///
    /// Every store has to say so: a screen taken while a tick is still
    /// collecting output would otherwise be serialized a second time by the
    /// tick's paced pass, and a resize's screen would leave the old window in
    /// charge of the new grid.
    pub(super) fn mark_stored(&mut self, pane: PaneId, now: Instant) {
        let pace = self.pace.entry(pane).or_default();
        pace.at = Some(now);
        pace.owed = 0;
    }

    /// Drop a pane's pace when its program switches screens. Bytes owed on the
    /// old screen must not drive a snapshot on the new one, and the window that
    /// bounded the old record says nothing about how recently the new one was
    /// stored.
    pub(super) fn reset_pace(&mut self, pane: PaneId) {
        self.pace.remove(&pane);
    }

    /// The screens worth taking at the end of a tick for the alternate-screen
    /// panes whose output moved them, and for the panes that still owe bytes
    /// after falling idle, and for which the record is at a clean boundary.
    ///
    /// At most once per pane per [`SNAPSHOT_INTERVAL`]: a busy program sends
    /// several chunks per tick, and serializing its grid on each of them is the
    /// most expensive thing the worker does — while the bytes it sends in
    /// between are exactly what a replay carries after the last screen, so
    /// waiting lengthens the replay rather than losing anything. A pane whose
    /// owed output has grown past [`SNAPSHOT_OWED_BYTES`] is taken early, which
    /// keeps that replay bounded between the worker's own crowded snapshots.
    ///
    /// The screens returned are accounted for here, so a caller only has to
    /// store them.
    pub(super) fn settle_restless(
        &mut self,
        restless: &[PaneId],
        now: Instant,
    ) -> Vec<(PaneId, Vec<u8>)> {
        let mut candidates: Vec<PaneId> = restless.to_vec();
        // A pane that went idle inside its window still owes the bytes it wrote
        // after its last screen. The caller only names panes that produced
        // output this tick, so without them a quiet pane would keep that tail
        // — and the snapshot state it depends on — indefinitely. Including
        // owed panes makes the latest write the one the interval is measured
        // from, whichever tick it happened on.
        for (&pane, pace) in &self.pace {
            if pace.owed > 0 && !candidates.contains(&pane) {
                candidates.push(pane);
            }
        }
        let mut screens = Vec::new();
        for pane in candidates {
            if !self.snapshot_due(pane, now) || !self.at_boundary(pane) {
                continue;
            }
            let Some(screen) = self.snapshot(pane) else {
                continue;
            };
            self.mark_stored(pane, now);
            screens.push((pane, screen));
        }
        screens
    }

    /// Whether this pane's screen is worth serializing at `now`.
    fn snapshot_due(&self, pane: PaneId, now: Instant) -> bool {
        let Some(pace) = self.pace.get(&pane) else {
            return true;
        };
        pace.owed > SNAPSHOT_OWED_BYTES
            || pace
                .at
                .is_none_or(|at| now.saturating_duration_since(at) >= SNAPSHOT_INTERVAL)
    }

    /// The earliest moment this tracker has timed work of its own: a synchronized
    /// update ending on the clock, or an owed pane whose window is still running.
    /// A pane that owes bytes with no screen stored yet is taken by the pass that
    /// recorded them, and one whose record ends mid-sequence waits for its next
    /// chunk rather than for a clock, so neither is named here.
    pub(super) fn next_deadline(&self) -> Option<Instant> {
        let sync = self
            .emulators
            .values()
            .filter_map(crate::runtime::emulator::PaneEmulator::sync_deadline)
            .min();
        let paced = self
            .pace
            .iter()
            .filter(|(pane, pace)| pace.owed > 0 && self.at_boundary(**pane))
            .filter_map(|(_, pace)| pace.at.map(|at| at + SNAPSHOT_INTERVAL))
            .min();
        sync.into_iter().chain(paced).min()
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
        self.pace.remove(&pane);
    }
}
