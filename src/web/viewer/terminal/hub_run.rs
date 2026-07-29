use super::TerminalHub;
use super::frame::{ServerMessage, TerminalFrame};
use super::hub_helpers::{
    Command, PaneState, StartupPane, broadcast_locked, canonical_order, push_scrollback,
};
use crate::backend::{BackendEvent, PaneId, PtyBackend, TerminalBackend};
use crate::web::viewer::limits;
use std::collections::VecDeque;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::Receiver;
use std::thread;
use std::time::Duration;

const POLL_INTERVAL: Duration = Duration::from_millis(8);

/// How a startup pane is named back to the client when it could not be opened.
/// The command text is the operator's own configuration and is what labels the
/// tab, so it is the name they would recognise.
fn startup_label(pane: &StartupPane) -> String {
    match pane.command.as_deref() {
        Some(command) => format!("`{command}`"),
        None => "a shell".to_string(),
    }
}

impl TerminalHub {
    pub(super) fn run(&self, cwd: &str, commands: Receiver<Command>, stop: Arc<AtomicBool>) {
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
                        // Slots reserved for a claimed startup set count here:
                        // the configured set has first refusal on them.
                        if !self.has_free_slot() {
                            self.send_error_to(client, "terminal limit reached");
                            continue;
                        }
                        match backend.open_pane(rows, cols, command.as_deref()) {
                            // Unnamed: a pane a client asked for is that client's
                            // to name, and the hub has nothing to add.
                            Ok(pane) => self.register_pane(pane, rows, cols, Some(client), None),
                            Err(err) => {
                                tracing::warn!(%err, "viewer: could not create a terminal");
                                self.send_error_to(client, "could not start a terminal");
                            }
                        }
                    }
                    Command::CreateStartup {
                        panes,
                        client,
                        reserved,
                    } => {
                        let mut held = reserved;
                        let mut remaining = panes.into_iter().peekable();
                        while let Some(pane) = remaining.next() {
                            // Spend this pane's own reservation first, so the
                            // check below sees the slot it is about to take as
                            // free rather than as still held for itself.
                            if held > 0 {
                                self.release_reserved(1);
                                held -= 1;
                            }
                            // The cap still binds. The reservation decides who
                            // gets a slot, not how many exist — a set larger
                            // than what was free at claim time comes up short
                            // here rather than overrunning the ceiling.
                            if !self.has_free_slot() {
                                // Name what did not start. The set is spent
                                // once claimed, so these will not run until
                                // the hub restarts — the user has to open them
                                // by hand, and cannot do that without knowing
                                // which ones they were.
                                let mut lost = vec![startup_label(&pane)];
                                lost.extend(remaining.map(|p| startup_label(&p)));
                                self.send_error_to(
                                    client,
                                    &format!(
                                        "terminal limit reached — {} did not start",
                                        lost.join(", ")
                                    ),
                                );
                                break;
                            }
                            match backend.open_pane(
                                pane.size.rows,
                                pane.size.cols,
                                pane.command.as_deref(),
                            ) {
                                // Registered as nobody's: the configured
                                // terminals belong to the session, not to
                                // whichever client happened to measure them
                                // first, so they must not pull that client's
                                // focus onto them.
                                Ok(id) => self.register_pane(
                                    id,
                                    pane.size.rows,
                                    pane.size.cols,
                                    None,
                                    pane.title.clone(),
                                ),
                                Err(err) => {
                                    tracing::warn!(%err, "viewer: could not start a terminal");
                                    self.send_error_to(
                                        client,
                                        &format!("could not start {}", startup_label(&pane)),
                                    );
                                }
                            }
                        }
                        // Whatever the break left holds slots nothing will fill.
                        self.release_reserved(held);
                    }
                    // Unknown pane ids are ignored rather than errored: a
                    // client racing a pane exit is normal, not an attack.
                    Command::Input { pane, data } if self.pane_is_live(pane) => {
                        let _ = backend.send_input(pane, &data);
                    }
                    Command::Resize {
                        pane,
                        rows,
                        cols,
                        client,
                    } => {
                        self.resize_pane(&mut backend, pane, rows, cols, client);
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
                    // Destroyed as well as forgotten. `PtyBackend` leaves pane
                    // removal to its caller (see its `drain_events`), so a pane
                    // that ended on its own — the user typed `exit`, or the
                    // command finished — would keep its entry, its PTY master,
                    // and its child handle for the hub's whole life. The cap
                    // counts live panes, not those, so open-and-exit in a loop
                    // accumulated descriptors with nothing to stop it.
                    BackendEvent::Exited { pane } => {
                        backend.destroy_pane(pane);
                        self.remove_pane_and_announce(pane);
                    }
                    // The hub owns its PTYs outright: it opens them through
                    // `open_pane`, which answers directly, and it is what
                    // decides their size and tells everyone. So none of these
                    // can come back the other way.
                    BackendEvent::Created { pane, .. } | BackendEvent::Resized { pane, .. } => {
                        tracing::debug!(pane, "hub: unexpected event from its own backend");
                    }
                    BackendEvent::SizeOwnership { .. } | BackendEvent::Reordered { .. } => {
                        tracing::debug!("hub: unexpected session event from its own backend");
                    }
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

    /// Whether another terminal fits under the cap, counting slots already
    /// held for a startup set that has been claimed but not created yet.
    fn has_free_slot(&self) -> bool {
        let state = self.state.lock().expect("terminal state poisoned");
        state.panes.len() + state.reserved < limits::MAX_PTYS_PER_REPO
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
    /// `client` is whoever asked for the pane, carried so that client alone can
    /// treat it as the one it opened. `None` for a pane nobody asked for.
    /// `title` is the name the session gives it, which only a configured startup
    /// terminal has.
    fn register_pane(
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
        state.panes.push(PaneState {
            id: pane,
            title,
            scrollback: VecDeque::new(),
            rows,
            cols,
        });
        if let Some(json) = json {
            broadcast_locked(&mut state.clients, TerminalFrame::Control(json));
        }
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
    fn resize_pane(
        &self,
        backend: &mut PtyBackend,
        pane: PaneId,
        rows: u16,
        cols: u16,
        client: u64,
    ) {
        let mut state = self.state.lock().expect("terminal state poisoned");
        // Not this client's to set. Dropped rather than refused: a client can
        // lose the sizing between laying out a frame and this arriving, which is
        // ordinary rather than an error worth interrupting anyone over.
        if state.size_owner != Some(client) {
            return;
        }
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
