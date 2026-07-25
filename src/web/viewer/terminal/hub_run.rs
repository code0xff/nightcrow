use super::TerminalHub;
use crate::backend::{BackendEvent, PaneId, PtyBackend, TerminalBackend};
use super::frame::{ServerMessage, TerminalFrame};
use super::hub_helpers::{
    Command, PaneState, broadcast_locked, canonical_order, push_scrollback,
};
use crate::web::viewer::limits;
use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::Receiver;
use std::sync::Arc;
use std::thread;
use std::time::Duration;

const POLL_INTERVAL: Duration = Duration::from_millis(8);

impl TerminalHub {
    pub(super) fn run(
        &self,
        cwd: &str,
        commands: Receiver<Command>,
        stop: Arc<AtomicBool>,
    ) {
        let mut backend = PtyBackend::new(cwd);

        while !stop.load(Ordering::Acquire) {
            while let Ok(command) = commands.try_recv() {
                match command {
                    Command::Create {
                        rows,
                        cols,
                        client,
                        command,
                    } => {
                        if self.pane_count() >= limits::MAX_PTYS_PER_REPO {
                            self.send_error_to(client, "terminal limit reached");
                            continue;
                        }
                        match backend.create_pane(rows, cols, command.as_deref()) {
                            Ok(pane) => self.register_pane(pane),
                            Err(err) => {
                                tracing::warn!(%err, "viewer: could not create a terminal");
                                self.send_error_to(client, "could not start a terminal");
                            }
                        }
                    }
                    // Unknown pane ids are ignored rather than errored: a
                    // client racing a pane exit is normal, not an attack.
                    Command::Input { pane, data } if self.pane_is_live(pane) => {
                        let _ = backend.send_input(pane, &data);
                    }
                    Command::Resize { pane, rows, cols } if self.pane_is_live(pane) => {
                        backend.resize(pane, rows, cols);
                    }
                    Command::Close { pane } if self.pane_is_live(pane) => {
                        backend.destroy_pane(pane);
                        self.remove_pane_and_announce(pane);
                    }
                    Command::Reorder { order } => self.reorder_panes(order),
                    _ => {}
                }
            }

            for event in backend.drain_events() {
                match event {
                    BackendEvent::Output { pane, data } => self.record_and_broadcast(pane, data),
                    BackendEvent::Exited { pane } => self.remove_pane_and_announce(pane),
                }
            }
            thread::sleep(POLL_INTERVAL);
        }

        let ids: Vec<PaneId> = self
            .state
            .lock()
            .expect("terminal state poisoned")
            .panes
            .iter()
            .map(|p| p.id)
            .collect();
        for pane in ids {
            backend.destroy_pane(pane);
        }
        // Drop the pane records too: the hub struct can outlive its worker
        // behind an `Arc`, and a late `connect` must not replay these now-dead
        // terminals.
        self.state
            .lock()
            .expect("terminal state poisoned")
            .panes
            .clear();
    }

    fn pane_count(&self) -> usize {
        self.state
            .lock()
            .expect("terminal state poisoned")
            .panes
            .len()
    }

    fn pane_is_live(&self, pane: PaneId) -> bool {
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
    fn register_pane(&self, pane: PaneId) {
        let json = serde_json::to_string(&ServerMessage::Created { pane }).ok();
        let mut state = self.state.lock().expect("terminal state poisoned");
        state.panes.push(PaneState {
            id: pane,
            scrollback: VecDeque::new(),
        });
        if let Some(json) = json {
            broadcast_locked(&mut state.clients, TerminalFrame::Control(json));
        }
    }

    /// Append output to the pane's bounded scrollback and broadcast it, both
    /// under one lock so a concurrently connecting client cannot slip a replay
    /// snapshot between the append and the broadcast.
    fn record_and_broadcast(&self, pane: PaneId, data: Vec<u8>) {
        let mut state = self.state.lock().expect("terminal state poisoned");
        if let Some(p) = state.panes.iter_mut().find(|p| p.id == pane) {
            push_scrollback(&mut p.scrollback, &data);
        }
        broadcast_locked(&mut state.clients, TerminalFrame::Output { pane, data });
    }

    /// Drop a pane and tell every client, but only if it was still live — a pane
    /// closed by command and then reported `Exited` by the backend must announce
    /// once, not twice.
    fn remove_pane_and_announce(&self, pane: PaneId) {
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
    fn reorder_panes(&self, order: Vec<PaneId>) {
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

    fn send_error_to(&self, client_id: u64, message: &str) {
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
            state.clients.remove(index);
        }
    }
}