//! Pane geometry and ordering: the two worker operations that change how the
//! panes are arranged rather than what is in them.
//!
//! Split out of `hub_run.rs` so the worker loop stays readable; the behaviour is
//! unchanged.

use super::TerminalHub;
use super::frame::{ServerMessage, TerminalFrame};
use super::hub_helpers::{broadcast_locked, canonical_order};
use crate::backend::{PaneId, PtyBackend, TerminalBackend};

impl TerminalHub {
    /// The size a pane's PTY is recorded as having, or `None` once the pane is
    /// gone. The one reader of that record outside the resize path itself, shared
    /// by everything that has to act on the size a pane actually has.
    pub(super) fn pane_size(&self, pane: PaneId) -> Option<(u16, u16)> {
        let state = self.state.lock().expect("terminal state poisoned");
        state
            .panes
            .iter()
            .find(|p| p.id == pane)
            .map(|p| (p.rows, p.cols))
    }

    /// Resize a live pane's PTY at the sizing owner's request, record the size it
    /// is now set to, and tell every client what it is.
    ///
    /// All under one lock, and the liveness check with them. `connect` reports
    /// each pane's size from this record and the client caches it as "already
    /// applied"; a client that slipped between the two would be told the old
    /// size for a PTY that has the new one, and would then skip the resize that
    /// would have corrected it. The `resize` itself is an ioctl on the master —
    /// far cheaper than the broadcast this lock already covers.
    pub(super) fn resize_pane(
        &self,
        backend: &mut PtyBackend,
        pane: PaneId,
        rows: u16,
        cols: u16,
        client: u64,
    ) {
        // Asked before the hub's lock, because the answer is the session's and
        // taking the two in the other order would invert the ordering `connect`
        // uses (hub lock, then ownership).
        let Some(connection) = self.connection_of(client) else {
            return;
        };
        // Not this client's to set. Dropped rather than refused: a client can
        // lose the sizing between laying out a frame and this arriving, which is
        // ordinary rather than an error worth interrupting anyone over.
        if !self.owns_size(connection) {
            return;
        }
        let mut state = self.state.lock().expect("terminal state poisoned");
        // An unknown pane is ignored rather than errored: a client racing a
        // pane exit is normal, not an attack.
        let Some(p) = state.panes.iter_mut().find(|p| p.id == pane) else {
            return;
        };
        if (p.rows, p.cols) == (rows, cols) {
            return;
        }
        backend.resize(pane, rows, cols);
        p.rows = rows;
        p.cols = cols;
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
    /// `order` is a full desired sequence of pane ids. It is reconciled
    /// against what is actually live so a reorder is robust to races with
    /// create/close: unknown ids are dropped and any live pane the request
    /// omits (e.g. one another client created in the same beat) is kept,
    /// appended in its current order (see [`canonical_order`]). The hub
    /// converges on that one canonical order and broadcasts it, so the
    /// sender and every other device end up with the same layout. Reordering
    /// only restyles the grid — pane ids, scrollback, and the live PTYs are
    /// untouched. A no-op reorder sends nothing.
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
