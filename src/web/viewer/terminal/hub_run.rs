use super::hub_modes::PaneModeTracker;
use super::hub_diag::ClearWatch;
use super::hub_repaint::Repaints;
use super::{DEFAULT_PANE_SIZE, TerminalHub};
use super::frame::{ServerMessage, TerminalFrame};
use super::hub_helpers::{Command, PaneState, broadcast_locked, push_scrollback};
use super::hub_plugins::Plugins;
use crate::runtime::emulator::PaneModes;
use crate::backend::{BackendEvent, PaneId, PtyBackend, TerminalBackend};
use crate::web::viewer::limits;
use std::collections::VecDeque;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::Receiver;
use std::thread;
use std::time::{Duration, Instant};

const POLL_INTERVAL: Duration = Duration::from_millis(8);

impl TerminalHub {
    pub(super) fn run(&self, cwd: &str, commands: Receiver<Command>, stop: Arc<AtomicBool>) {
        let mut backend = PtyBackend::new(cwd);
        // Before the loop, because a pane can be created on the first iteration
        // and a plugin has to exist to be told about it. Only the plugins some
        // configured pane opted into are launched (see `Plugins::start`).
        let mut plugins = Plugins::start(cwd, &self.plugins, &self.startup);
        // What each pane's program has done to its terminal, so a client that
        // attaches later can be told rather than left to infer it from a replay
        // the setup bytes fell out of.
        let mut modes = PaneModeTracker::default();
        // Repaints asked for by attaching clients, and the sizes owed back.
        let mut repaints = Repaints::default();
        // Why this is here at all: `hub_diag`.
        let mut clears = ClearWatch::default();

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
                            // to name, and the hub has nothing to add. No plugin
                            // association either, ever — a shell a client opened
                            // is nobody's to drive but the person at it.
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
                        self.open_startup_panes(&mut backend, &mut plugins, panes, client, reserved)
                    }
                    // Unknown pane ids are ignored rather than errored: a
                    // client racing a pane exit is normal, not an attack.
                    Command::Input { pane, data, client } if self.pane_is_live(pane) => {
                        clears.note_input(pane, client, &data, Instant::now());
                        // A person at the keyboard has taken the pane back, so
                        // its plugin is told and everything it had planned for
                        // the pane is dropped — before the bytes land, so the
                        // cancellation cannot be overtaken by a decision made
                        // from the output this input produces.
                        plugins.user_input(&backend, pane);
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
                        // Closed for good, unlike an exit: the slot goes with
                        // the process, so there is nothing left to relaunch.
                        if plugins.owner(pane).is_some() {
                            plugins.pane_closed(&backend, pane);
                            plugins.forget(&backend, pane);
                            backend.retire_slot(pane);
                            self.end_recovery(pane);
                        }
                        backend.destroy_pane(pane);
                        self.remove_pane_and_announce(pane);
                    }
                    Command::Reorder { order } => self.reorder_panes(order),
                    // Deliberately not gated on the pane being live: a pane with
                    // a recovery pending is one whose process has already ended,
                    // so it is no longer in the client-visible list.
                    Command::CancelRecovery { pane } => {
                        self.cancel_recovery(&mut backend, &mut plugins, pane)
                    }
                    Command::Repaint { panes } => {
                        self.start_repaints(&mut backend, &mut repaints, &panes, Instant::now())
                    }
                    Command::ReloadPlugins { plugins: configs } => {
                        self.reload_hub_plugins(&mut backend, &mut plugins, &configs)
                    }
                    _ => {}
                }
            }

            for event in backend.drain_events() {
                match event {
                    BackendEvent::Output { pane, data } => {
                        plugins.pane_output(&backend, pane, &data);
                        // Before the lock: opening a pane's tracking emulator
                        // reads the shared state for its size.
                        let modes = modes.observe(pane, &data, || {
                            self.pane_size(pane)
                                .unwrap_or((DEFAULT_PANE_SIZE.rows, DEFAULT_PANE_SIZE.cols))
                        });
                        self.record_and_broadcast(pane, data, modes);
                    }
                    // Destroyed as well as forgotten. `PtyBackend` leaves pane
                    // removal to its caller (see its `drain_events`), so a pane
                    // that ended on its own — the user typed `exit`, or the
                    // command finished — would keep its entry, its PTY master,
                    // and its child handle for the hub's whole life. The cap
                    // counts live panes, not those, so open-and-exit in a loop
                    // accumulated descriptors with nothing to stop it.
                    BackendEvent::Exited { pane } => {
                        modes.forget(pane);
                        clears.forget(pane);
                        self.forget_repaints(&mut repaints, pane);
                        self.pane_exited(&mut backend, &mut plugins, pane)
                    }
                    // The hub owns its PTYs outright: it opens them through
                    // `open_pane`, which answers directly, and it is what
                    // decides their size and tells everyone. So none of these
                    // can come back the other way.
                    BackendEvent::Created { pane, .. } | BackendEvent::Resized { pane, .. } => {
                        tracing::debug!(pane, "hub: unexpected event from its own backend");
                    }
                    BackendEvent::SizeOwnership { .. }
                    | BackendEvent::Reordered { .. }
                    | BackendEvent::Recovery { .. } => {
                        tracing::debug!("hub: unexpected session event from its own backend");
                    }
                }
            }

            if !repaints.is_idle() {
                self.finish_repaints(&mut backend, &mut repaints, Instant::now());
            }

            if !plugins.is_inert() {
                let now = Instant::now();
                plugins.notify_idle(&backend, now);
                self.dispatch_plugin_commands(&mut backend, &mut plugins, now);
                // Told to the clients, or one that was shown a deadline keeps
                // counting down to a moment that has already passed.
                for pane in plugins.expire_pending(&mut backend, now) {
                    self.end_recovery(pane);
                }
            }
            thread::sleep(POLL_INTERVAL);
        }

        // Ahead of the panes: a plugin child is not one of `PtyBackend`'s panes,
        // so this is the only place it is ever reaped, and telling it to stop
        // before its panes disappear beneath it is the courteous order.
        plugins.shutdown();

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
    pub(super) fn has_free_slot(&self) -> bool {
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
    fn record_and_broadcast(&self, pane: PaneId, data: Vec<u8>, modes: PaneModes) {
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
            state.clients.remove(index);
        }
    }
}
