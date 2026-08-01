//! The hub's pane records: adding one, appending to it, dropping it, and the
//! two questions the worker asks about the list before it acts.
//!
//! Every one of these pairs a change to `Shared` with the broadcast that
//! announces it, under a single lock — that pairing is what keeps a client
//! connecting mid-change from seeing a pane twice or not at all (see
//! [`Shared`](super::hub_helpers::Shared)). Split out of `hub_run.rs` so that
//! file is the worker loop and nothing else; the behaviour is unchanged.

use super::TerminalHub;
use super::frame::{ServerMessage, TerminalFrame};
use super::hub_helpers::{PaneState, broadcast_locked, push_scrollback};
use super::hub_zoom::clear_zoom_locked;
use crate::backend::PaneId;
use crate::runtime::emulator::PaneModes;
use crate::web::viewer::limits;
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
    /// treat it as the one it opened. `None` for a pane nobody asked for.
    /// `title` is the name the session gives it, which only a configured startup
    /// terminal has.
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
        // panel gives way to the one about to open.
        //
        // Ahead of the announcement rather than after it, though both go out
        // under this one lock. They are two frames, and a client renders between
        // them: told about the pane while still zoomed past it, it spends that
        // render with the new terminal hidden — and moves the keyboard onto the
        // pane filling the panel instead of the one it just asked for.
        clear_zoom_locked(&mut state);
        state.panes.push(PaneState {
            id: pane,
            title,
            scrollback: VecDeque::new(),
            rows,
            cols,
            modes: PaneModes::default(),
        });
        if let Some(json) = json {
            broadcast_locked(&mut state.clients, TerminalFrame::Control(json));
        }
    }

    /// Append output to the pane's bounded scrollback, record the terminal state
    /// it left the pane in, and broadcast it — all under one lock so a
    /// concurrently connecting client cannot slip a replay snapshot between the
    /// append and the broadcast.
    pub(super) fn record_and_broadcast(&self, pane: PaneId, data: Vec<u8>, modes: PaneModes) {
        let mut state = self.state.lock().expect("terminal state poisoned");
        if let Some(p) = state.panes.iter_mut().find(|p| p.id == pane) {
            push_scrollback(&mut p.scrollback, &data);
            p.modes = modes;
        }
        broadcast_locked(&mut state.clients, TerminalFrame::Output { pane, data });
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
