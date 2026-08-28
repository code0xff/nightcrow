use super::{PendingPaneResize, TerminalState};
use crate::backend::{PaneId, ResizeOutcome};
use std::time::{Duration, Instant};

const RESIZE_RETRY_INTERVAL: Duration = Duration::from_millis(100);

impl TerminalState {
    /// Fit each visible pane to its rendered cells. Remote backends confirm the
    /// applied size asynchronously, so desired, pending, and confirmed geometry
    /// remain distinct until a `Resized` event arrives.
    pub fn resize_visible_panes(&mut self, layouts: &[(PaneId, u16, u16)]) {
        self.resize_visible_panes_at(layouts, Instant::now());
    }

    pub(crate) fn resize_visible_panes_at(&mut self, layouts: &[(PaneId, u16, u16)], now: Instant) {
        let active_id = self.active_pane_id();
        for &(id, rows, cols) in layouts {
            let size = crate::runtime::emulator::effective_size(rows, cols);
            if Some(id) == active_id {
                self.size = size;
            }
            if !self.owns_size {
                continue;
            }
            self.last_content_size.insert(id, size);
            if self.confirmed_content_size.get(&id) == Some(&size) {
                self.pending_content_size.remove(&id);
                continue;
            }
            let retry_due = self.pending_content_size.get(&id).is_none_or(|pending| {
                pending.size != size
                    || now.saturating_duration_since(pending.attempted_at) >= RESIZE_RETRY_INTERVAL
            });
            if retry_due {
                self.request_resize(id, size, now);
            }
        }
    }

    fn request_resize(&mut self, id: PaneId, size: (u16, u16), now: Instant) {
        let outcome = self
            .backend
            .as_mut()
            .map(|backend| backend.resize(id, size.0, size.1))
            .unwrap_or(Ok(ResizeOutcome::Applied));
        match outcome {
            Ok(ResizeOutcome::Applied) => {
                if let Some(emulator) = self.emulators.get_mut(&id) {
                    emulator.resize(size.0, size.1);
                }
                self.confirmed_content_size.insert(id, size);
                self.pending_content_size.remove(&id);
            }
            Ok(ResizeOutcome::Pending) => {
                self.note_resize_attempt(id, size, now);
            }
            Err(err) => {
                tracing::warn!(%err, pane = id, rows = size.0, cols = size.1, "could not resize a terminal pane");
                self.note_resize_attempt(id, size, now);
            }
        }
    }

    fn note_resize_attempt(&mut self, id: PaneId, size: (u16, u16), now: Instant) {
        self.pending_content_size.insert(
            id,
            PendingPaneResize {
                size,
                attempted_at: now,
            },
        );
    }

    pub(super) fn confirm_resize(&mut self, pane: PaneId, rows: u16, cols: u16) {
        let Some(emulator) = self.emulators.get_mut(&pane) else {
            return;
        };
        let size = crate::runtime::emulator::effective_size(rows, cols);
        emulator.resize(size.0, size.1);
        self.confirmed_content_size.insert(pane, size);
        // A matching ACK completes the request; an older ACK also clears it so
        // the desired/confirmed mismatch is retried immediately next frame.
        self.pending_content_size.remove(&pane);
        if !self.owns_size {
            self.last_content_size.insert(pane, size);
        }
    }
}
