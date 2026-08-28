//! The hub's pane records: adding one, appending to it, dropping it, and the
//! two questions the worker asks about the list before it acts.
//!
//! Every one of these pairs a change to `Shared` with the broadcast that
//! announces it, under a single lock — that pairing is what keeps a client
//! connecting mid-change from seeing a pane twice or not at all (see
//! [`Shared`](super::hub_helpers::Shared)).

use super::TerminalHub;
use super::frame::{ServerMessage, TerminalFrame};
use super::hub_helpers::{PaneState, broadcast_locked, push_scrollback};
use super::hub_modes::Observed;
use super::hub_zoom::clear_zoom_locked;
use crate::backend::PaneId;
use crate::runtime::emulator::PaneModes;
use crate::session::limits;
use std::collections::VecDeque;

impl TerminalHub {
    /// Whether another terminal fits under the cap, counting slots already
    /// held for a startup set that has been claimed but not created yet.
    pub(super) fn has_free_slot(&self) -> bool {
        let state = self.state.lock().expect("terminal state poisoned");
        state.panes.len() + state.reserved < limits::MAX_PTYS_PER_REPO
    }

    pub(super) fn pane_is_live(&self, pane: PaneId) -> bool {
        self.state
            .lock()
            .expect("terminal state poisoned")
            .panes
            .iter()
            .any(|p| p.id == pane)
    }

    /// Record a new pane and announce it to every client. Broadcasting under the
    /// same lock that adds the pane keeps it consistent with `connect`'s replay:
    /// a client either sees this pane via `connect` or via this broadcast, never
    /// both and never neither.
    /// `client` is whoever asked for the pane, carried so that client alone can
    /// treat it as the one it opened. `title` is the name the session gives it,
    /// which only a configured startup terminal has.
    pub(super) fn register_pane(
        &self,
        pane: PaneId,
        rows: u16,
        cols: u16,
        client: Option<u64>,
        title: Option<String>,
    ) {
        let json = serde_json::to_string(&ServerMessage::Created {
            pane,
            rows,
            cols,
            client,
            title: title.clone(),
        })
        .ok();
        let mut state = self.state.lock().expect("terminal state poisoned");
        // A pane nobody can see is not a terminal, so whatever was filling the
        // panel gives way to the one about to open. Ahead of the announcement:
        // they are two frames and a client renders between them — told about
        // the pane while still zoomed past it, it spends that render with the
        // new terminal hidden and its keyboard on the wrong pane.
        clear_zoom_locked(&mut state);
        state.panes.push(PaneState {
            id: pane,
            title,
            scrollback: VecDeque::new(),
            normal_screen: Vec::new(),
            covered: 0,
            screen: Vec::new(),
            since: VecDeque::new(),
            rows,
            cols,
            modes: PaneModes::default(),
        });
        if let Some(json) = json {
            broadcast_locked(&mut state.clients, TerminalFrame::Control(json));
        }
    }

    /// Record output against the pane and broadcast it — under one lock, so a
    /// concurrently connecting client cannot slip a replay between the record
    /// and the broadcast and end up with the chunk missing or doubled.
    ///
    /// Where the output is recorded depends on the mode the chunk leaves the
    /// pane in (see [`PaneState`](super::hub_helpers::PaneState)). `screen` is
    /// the serialized screen when the caller has one to hand over, which it
    /// takes before locking — the emulator it comes from is not `Send`.
    ///
    /// Returns how many recorded bytes a fresh snapshot would supersede — the
    /// uncovered tail on the normal screen (see [`push_scrollback`]), `since`
    /// on the alternate one. The worker reads the pane's appetite for a
    /// snapshot off this count (crowded past the cap, desperate well past it).
    ///
    /// Attached clients are not told a new title: they are being handed the
    /// very bytes that set it, and each runs the emulator that reads them.
    /// This record is for the client that is not here yet.
    pub(super) fn record_and_broadcast(
        &self,
        pane: PaneId,
        data: Vec<u8>,
        observed: Observed,
        screen: Option<Vec<u8>>,
    ) -> usize {
        let mut state = self.state.lock().expect("terminal state poisoned");
        let mut owed = 0;
        if let Some(p) = state.panes.iter_mut().find(|p| p.id == pane) {
            if observed.modes.alt_screen {
                match screen {
                    // The snapshot already accounts for this chunk, so nothing is
                    // owed after it.
                    Some(screen) => {
                        p.screen = screen;
                        p.since.clear();
                    }
                    None => {
                        p.since.extend(data.iter().copied());
                        owed = p.since.len();
                    }
                }
            } else {
                // Back on the normal screen — or never left it — so the normal
                // record is in charge again and whatever was kept for the
                // alternate one is spent. The frozen `normal_screen` + `covered`
                // stay: the program's normal grid was preserved across the
                // alternate screen, so they are as valid as when it left.
                if observed.alt_changed {
                    p.screen = Vec::new();
                    p.since.clear();
                }
                owed = push_scrollback(&mut p.scrollback, &mut p.covered, &data);
            }
            p.modes = observed.modes;
            if let Some(title) = observed.title {
                p.title = Some(title);
            }
        }
        broadcast_locked(&mut state.clients, TerminalFrame::Output { pane, data });
        owed
    }

    /// Fold into a pane's record a synchronized update that ended on the clock
    /// instead of on a byte.
    ///
    /// No output arrived — the record already holds the bytes the emulator was
    /// keeping off its grid — but the grid has caught up with them, so what it
    /// says about the pane supersedes what the record was last told. A pane
    /// that has gone since is ignored.
    pub(super) fn store_settled(&self, pane: PaneId, observed: Observed) {
        let mut state = self.state.lock().expect("terminal state poisoned");
        let Some(p) = state.panes.iter_mut().find(|p| p.id == pane) else {
            return;
        };
        // Coming off the alternate screen spends what was kept for it, exactly
        // as a chunk carrying the switch would (see `record_and_broadcast`).
        if observed.alt_changed && !observed.modes.alt_screen {
            p.screen = Vec::new();
            p.since.clear();
        }
        p.modes = observed.modes;
        if let Some(title) = observed.title {
            p.title = Some(title);
        }
    }

    /// Replace a pane's recorded screen, forgetting what was owed on top of the
    /// one before it. A pane that has gone since the snapshot was taken is ignored.
    pub(super) fn store_screen(&self, pane: PaneId, screen: Vec<u8>) {
        let mut state = self.state.lock().expect("terminal state poisoned");
        if let Some(p) = state.panes.iter_mut().find(|p| p.id == pane) {
            p.screen = screen;
            p.since.clear();
        }
    }

    /// Replace a pane's normal-screen snapshot, marking everything recorded so
    /// far as covered by it. Correct only because the worker is the sole writer
    /// of output: the snapshot the caller took has seen exactly the chunks the
    /// ring holds, so the mark lands on a chunk boundary. A pane that has gone
    /// since is ignored.
    pub(super) fn store_normal_screen(&self, pane: PaneId, screen: Vec<u8>) {
        let mut state = self.state.lock().expect("terminal state poisoned");
        if let Some(p) = state.panes.iter_mut().find(|p| p.id == pane) {
            p.covered = p.scrollback.len();
            p.normal_screen = screen;
        }
    }

    /// Drop a pane and tell every client, but only if it was still live — a pane
    /// closed by command and then reported `Exited` by the backend must announce
    /// once, not twice.
    pub(super) fn remove_pane_and_announce(&self, pane: PaneId) {
        let json = serde_json::to_string(&ServerMessage::Exited { pane }).ok();
        let mut state = self.state.lock().expect("terminal state poisoned");
        let existed = state.panes.iter().any(|p| p.id == pane);
        if !existed {
            return;
        }
        state.panes.retain(|p| p.id != pane);
        if let Some(json) = json {
            broadcast_locked(&mut state.clients, TerminalFrame::Control(json));
        }
        // The zoomed pane is the one that just left, so the panel has nothing to
        // fill it with.
        if state.zoomed == Some(pane) {
            clear_zoom_locked(&mut state);
        }
    }

    pub(super) fn send_error_to(&self, client_id: u64, message: &str) {
        let Ok(json) = serde_json::to_string(&ServerMessage::Error {
            message: message.to_string(),
        }) else {
            return;
        };
        let mut state = self.state.lock().expect("terminal state poisoned");
        if let Some(index) = state.clients.iter().position(|c| c.id == client_id)
            && state.clients[index]
                .tx
                .try_send(TerminalFrame::Control(json))
                .is_err()
        {
            state.clients[index].cut_off();
            state.clients.remove(index);
        }
    }
}
