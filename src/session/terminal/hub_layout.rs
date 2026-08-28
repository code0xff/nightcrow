//! Pane geometry and ordering: the two worker operations that change how the
//! panes are arranged rather than what is in them. Split out of `hub_run.rs` so
//! the worker loop stays readable.

use super::TerminalHub;
use super::frame::{ServerMessage, TerminalFrame};
use super::hub_helpers::{PendingResize, broadcast_locked, canonical_order};
use crate::backend::{PaneId, PtyBackend, TerminalBackend};

impl TerminalHub {
    /// Keep only the newest requested size for this connection and pane.
    pub(super) fn queue_resize(
        &self,
        pane: PaneId,
        rows: u16,
        cols: u16,
        client: u64,
        connection: u64,
    ) {
        // Validate without holding the hub state lock: `connect` takes it before
        // ownership, while this path takes the resize queue before ownership.
        // Besides dropping an ordinary close race, this bounds the latest-value
        // map to live panes even when a client sends arbitrary ids.
        if !self.pane_is_live(pane) {
            return;
        }
        let mut pending = self
            .pending_resizes
            .lock()
            .expect("terminal resize queue poisoned");
        // Checked while holding the queue lock so `disconnect` cannot purge and
        // then have this request inserted behind it.
        if !self.owns_size(connection) {
            return;
        }
        pending.insert(
            (connection, pane),
            PendingResize {
                pane,
                rows,
                cols,
                client,
                connection,
            },
        );
    }

    pub(super) fn discard_pending_resizes(&self, connection: u64) {
        self.pending_resizes
            .lock()
            .expect("terminal resize queue poisoned")
            .retain(|(queued_connection, _), _| *queued_connection != connection);
    }

    pub(super) fn take_pending_resizes(&self) -> Vec<PendingResize> {
        std::mem::take(
            &mut *self
                .pending_resizes
                .lock()
                .expect("terminal resize queue poisoned"),
        )
        .into_values()
        .collect()
    }

    /// The size a pane's PTY is recorded as having, or `None` once the pane is
    /// gone.
    pub(super) fn pane_size(&self, pane: PaneId) -> Option<(u16, u16)> {
        let state = self.state.lock().expect("terminal state poisoned");
        state
            .panes
            .iter()
            .find(|p| p.id == pane)
            .map(|p| (p.rows, p.cols))
    }

    /// Resize a live pane's PTY at the sizing owner's request, record the size,
    /// and tell every client what it is. All under one lock, with the liveness
    /// check — `connect` reports each pane's size from this record and the
    /// client caches it as "already applied"; a client that slipped between
    /// the two would be told the old size for a PTY that has the new one, and
    /// would then skip the resize that would have corrected it. `modes` is
    /// resized with the PTY: the grid a connecting client's screen is read
    /// from has to wrap where the child now does (see
    /// [`hub_modes`](super::hub_modes)).
    pub(super) fn resize_pane(
        &self,
        backend: &mut PtyBackend,
        modes: &mut super::hub_modes::PaneModeTracker,
        resize: PendingResize,
    ) {
        let PendingResize {
            pane,
            rows,
            cols,
            client,
            connection,
        } = resize;
        let mut state = self.state.lock().expect("terminal state poisoned");
        // The queue may already have handed this value to the worker when its
        // connection departs. Validate its original registration and ownership
        // together while holding state, in the same state -> ownership order as
        // `connect`, so a replacement owner cannot inherit the stale request.
        if !self.client_owns_size(&state, client, connection) {
            return;
        }
        // An unknown pane is ignored rather than errored: a client racing a
        // pane exit is normal.
        let Some(p) = state.panes.iter_mut().find(|p| p.id == pane) else {
            return;
        };
        let changed = (p.rows, p.cols) != (rows, cols);
        if changed {
            if let Err(err) = backend.resize(pane, rows, cols) {
                tracing::warn!(%err, pane, rows, cols, "could not resize a session PTY");
                return;
            }
            modes.resize(pane, rows, cols);
            p.rows = rows;
            p.cols = cols;
        }
        // The grid just reflowed, so a snapshot taken before it wraps where the
        // child no longer does. Refreshed into whichever record the pane is on
        // — the emulator's active grid is that screen. Skipped when the last
        // chunk ended mid-sequence (`at_boundary`): a snapshot anchored there
        // would splice into an open sequence on replay, and a stale-size screen
        // is the smaller harm — the next output refreshes it.
        if changed
            && modes.at_boundary(pane)
            && let Some(screen) = modes.snapshot(pane)
        {
            if p.modes.alt_screen {
                p.screen = screen;
                p.since.clear();
            } else {
                p.covered = p.scrollback.len();
                p.normal_screen = screen;
            }
        }
        // Every client's emulator has to wrap where the child now does, so the
        // size it was actually set to goes to all of them — including the one
        // that asked, which learns here if its request was clamped.
        if let Ok(json) = serde_json::to_string(&ServerMessage::Resized { pane, rows, cols }) {
            broadcast_locked(&mut state.clients, TerminalFrame::Control(json));
        }
    }

    /// Reorder the live panes to match `order` and tell every client the
    /// result.
    ///
    /// `order` is reconciled against what is actually live so a reorder is
    /// robust to races with create/close (see [`canonical_order`]). The hub
    /// converges on that one canonical order and broadcasts it, so the sender
    /// and every other device end up with the same layout. A no-op reorder
    /// sends nothing.
    pub(super) fn reorder_panes(&self, order: Vec<PaneId>) {
        let mut state = self.state.lock().expect("terminal state poisoned");
        let before: Vec<PaneId> = state.panes.iter().map(|p| p.id).collect();
        let target = canonical_order(&before, &order);
        if target == before {
            return;
        }
        // `target` is a permutation of `before`, so every id resolves and `old`
        // ends empty. Move each `PaneState` rather than clone it — it owns the
        // pane's scrollback.
        let mut old = std::mem::take(&mut state.panes);
        let mut reordered = Vec::with_capacity(old.len());
        for id in &target {
            if let Some(pos) = old.iter().position(|p| p.id == *id) {
                reordered.push(old.remove(pos));
            }
        }
        state.panes = reordered;
        if let Ok(json) = serde_json::to_string(&ServerMessage::Reordered { order: target }) {
            broadcast_locked(&mut state.clients, TerminalFrame::Control(json));
        }
    }
}
