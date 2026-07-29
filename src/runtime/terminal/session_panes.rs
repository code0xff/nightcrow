//! Closing and reordering panes: the operations the session owns.
//!
//! Both are requests. The pane list is the session's, so it changes when the
//! session says it did — which is also how a close or a drag in another client
//! arrives here. Only what this client is looking at is decided locally, and
//! that follows the *pane* rather than the slot.

use crate::backend::PaneId;
use crate::runtime::terminal::{PaneInfo, TerminalState};

impl TerminalState {
    /// Ask for the active pane to be closed. Reports whether there was one to
    /// ask about; an empty list is a benign no-op.
    ///
    /// A request, like a create. The pane goes when the session says it did
    /// ([`BackendEvent::Exited`]), which is also how a pane someone else closed
    /// arrives. Removing it here instead would show it gone while its process
    /// kept running — and a close the session never carried out (a full command
    /// queue drops one) would leave this client unable to see that pane again.
    pub fn close_active(&mut self) -> bool {
        let Some(info) = self.panes.get(self.active) else {
            return false;
        };
        let id = info.id;
        match &mut self.backend {
            Some(backend) => backend.destroy_pane(id),
            None => return false,
        }
        true
    }

    /// Ask for the active pane to be closed and take delivery of it in one step.
    /// Only for tests; every fake backend reports the exit immediately, so one
    /// poll applies it.
    #[cfg(test)]
    pub fn close_active_now(&mut self) -> bool {
        let asked = self.close_active();
        self.poll();
        asked
    }

    /// Ask for the active pane and the pane at `idx` to trade places.
    ///
    /// A request, not a move: the order belongs to the session, so it is applied
    /// when it comes back as [`BackendEvent::Reordered`] — for every client at
    /// once, rather than here alone. Returns `true` when a request was made and
    /// `false` for an out-of-range `idx` or a self-swap (both benign no-ops).
    pub fn swap_active_with(&mut self, idx: usize) -> bool {
        if idx >= self.panes.len() || idx == self.active {
            return false;
        }
        let mut order: Vec<PaneId> = self.panes.iter().map(|pane| pane.id).collect();
        order.swap(self.active, idx);
        match &mut self.backend {
            Some(backend) => backend.reorder(&order),
            None => return false,
        }
        true
    }

    /// Put the panes in the order the session gives.
    ///
    /// Reconciled rather than applied blindly, because the client and the session
    /// can disagree for a beat: an id this client has not adopted yet is skipped,
    /// and a pane the order omits keeps its place at the end. Focus follows the
    /// *pane* it was on rather than the slot — the point of a swap is to move a
    /// pane while still looking at it. Per-pane state (emulators, scroll, sizes,
    /// prompt buffers) is keyed by id, so none of it moves.
    ///
    /// Test-only for a locally-backed state, which has no session to be told by;
    /// [`swap_active_with`](Self::swap_active_with) is what asks in production.
    pub(crate) fn apply_order(&mut self, order: &[PaneId]) {
        let active_id = self.active_pane_id();
        let mut taken: Vec<PaneInfo> = Vec::with_capacity(self.panes.len());
        for id in order {
            if let Some(index) = self.panes.iter().position(|pane| pane.id == *id) {
                taken.push(self.panes.remove(index));
            }
        }
        taken.append(&mut self.panes);
        self.panes = taken;
        self.active = active_id
            .and_then(|id| self.panes.iter().position(|pane| pane.id == id))
            .unwrap_or(self.active)
            .min(self.panes.len().saturating_sub(1));
        self.sync_visible_window();
    }

    /// Ask for a swap and take delivery of it in one step. Only for tests, which
    /// should not have to spell the round trip out; every fake backend echoes the
    /// order immediately, so one poll applies it.
    #[cfg(test)]
    pub fn swap_active_with_now(&mut self, idx: usize) -> bool {
        let asked = self.swap_active_with(idx);
        self.poll();
        asked
    }
}
