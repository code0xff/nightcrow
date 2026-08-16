//! Keeping panes repainting when a program abandons a synchronized update.
//!
//! An emulator holds a synchronized update's bytes back from its grid until the
//! update ends, and a program killed mid-frame never ends it (see
//! `runtime::emulator::sync`). Nothing else on this side ticks, so the poll
//! that drains the backend closes such updates on the clock — otherwise the
//! pane stops repainting while its shell happily keeps taking input.

use crate::backend::PaneId;
use crate::runtime::emulator::EmulatorEvents;
use std::time::Instant;

use super::TerminalState;

impl TerminalState {
    /// Route what an emulator produced while processing: a window title to the
    /// pane's tab, and terminal query replies back to the program that asked.
    ///
    /// Replies bypass [`send_input`](Self::send_input) on purpose: an
    /// emulator-generated answer must not clear the user's scroll position or
    /// land in the prompt log.
    pub(super) fn apply_emulator_events(&mut self, pane: PaneId, events: EmulatorEvents) {
        if let Some(title) = events.title
            && let Some(info) = self.panes.iter_mut().find(|p| p.id == pane)
        {
            info.title = title;
        }
        if !events.pty_writes.is_empty()
            && let Some(backend) = &mut self.backend
            && let Err(e) = backend.send_input(pane, &events.pty_writes)
        {
            tracing::warn!("failed to send terminal reply to pane {pane}: {e}");
        }
    }

    /// End every synchronized update that has outlived its timeout as of
    /// `now`, applying the bytes it was holding back.
    pub(super) fn settle_sync_updates(&mut self, now: Instant) {
        let expired: Vec<PaneId> = self
            .emulators
            .iter()
            .filter(|(_, emulator)| emulator.sync_expired(now))
            .map(|(id, _)| *id)
            .collect();
        for pane in expired {
            let Some(emulator) = self.emulators.get_mut(&pane) else {
                continue;
            };
            let events = emulator.settle_sync();
            self.apply_emulator_events(pane, events);
        }
    }
}
