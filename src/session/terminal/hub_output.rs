//! One chunk of a pane's output: the tracking emulator that reads it, the record that
//! keeps it, and the snapshot the chunk itself calls for.
//!
//! The per-chunk half of the worker's tick. What it decides about a pane's pace is
//! carried back as a verdict, which the tick's end-of-turn pass acts on.

use super::hub_modes::PaneModeTracker;
use super::{DEFAULT_PANE_SIZE, TerminalHub};
use crate::backend::PaneId;
use crate::session::limits;

impl TerminalHub {
    /// Feed one chunk of a pane's output through its tracking emulator, record it,
    /// and take the snapshot the chunk itself calls for.
    ///
    /// Returns whether the pane is left restless — on the alternate screen, and
    /// without a screen taken from this chunk — for the end-of-tick pass.
    pub(super) fn record_pane_output(
        &self,
        pane: PaneId,
        data: Vec<u8>,
        modes: &mut PaneModeTracker,
    ) -> bool {
        // Before the lock: opening a pane's tracking emulator reads the shared
        // state for its size.
        let observed = modes.observe(pane, &data, || {
            self.pane_size(pane)
                .unwrap_or((DEFAULT_PANE_SIZE.rows, DEFAULT_PANE_SIZE.cols))
        });
        let alt = observed.modes.alt_screen;
        // A chunk that moved the pane onto the alternate screen carries the
        // switch and the first paint together, so its screen is taken in the
        // same breath as the record — provided the chunk ends clean
        // (`at_boundary`, below). Cut mid-sequence it is filed into `since`
        // instead, and the tick's restless pass takes the screen once the stream
        // closes; until then a connecting client replays `since` raw, landing
        // pre-switch text on the wrong buffer for that moment — the paint that
        // follows covers it, and the retry replaces it.
        let screen = (alt && observed.alt_changed && modes.at_boundary(pane))
            .then(|| modes.snapshot(pane))
            .flatten();
        let owed = self.record_and_broadcast(pane, data, observed, screen);
        if alt {
            modes.note_owed(pane, owed);
        }
        // Snapshots wait for a chunk that ends with its sequences closed
        // (`at_boundary`): the snapshot is spliced into the recorded stream on
        // replay, and a seam inside a sequence hands a reattaching client its
        // tail as ordinary input. A crowded record retries with every next
        // chunk; a desperate one has waited a whole extra ring, which no real
        // sequence spans, so the records are bounded over a torn seam.
        // Desperation overrides the sequence seam only — a grid missing a
        // synchronized update's bytes (`screen_current`) must never be
        // snapshotted, and needs no override: the update ends at the processor's
        // own buffer cap if nothing else.
        let crowded = owed > limits::MAX_TERMINAL_SCROLLBACK_BYTES;
        let desperate = owed > 2 * limits::MAX_TERMINAL_SCROLLBACK_BYTES;
        let ready = modes.at_boundary(pane) || (desperate && modes.screen_current(pane));
        if !alt {
            if crowded && ready {
                // Not per tick like the alternate screen's: between snapshots
                // the tail keeps the replay exact on its own, so this costs a
                // serialization once per ring's worth of output.
                if let Some(screen) = modes.snapshot(pane) {
                    self.store_normal_screen(pane, screen);
                }
            }
            return false;
        }
        if crowded && ready {
            // Refreshed now rather than at the end of the tick: what a
            // connecting client is handed on top of the screen has to stay
            // bounded, and only a fresh screen bounds it. The record owes
            // nothing after it, so the paced pass has nothing to add.
            if let Some(screen) = modes.snapshot(pane) {
                self.store_screen(pane, screen);
                modes.note_owed(pane, 0);
            }
            return false;
        }
        true
    }
}
