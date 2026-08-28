use super::frame::{ServerMessage, TerminalFrame};
use super::hub_diag::ClearWatch;
use super::hub_helpers::{Command, broadcast_locked};
use super::hub_modes::PaneModeTracker;
use super::hub_plugins::Plugins;
use super::{DEFAULT_PANE_SIZE, TerminalHub};
use crate::backend::{BackendEvent, PaneId, PtyBackend, TerminalBackend};
use crate::session::limits;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::Receiver;
use std::thread;
use std::time::{Duration, Instant};

const POLL_INTERVAL: Duration = Duration::from_millis(8);
const COMMANDS_BETWEEN_RESIZES: usize = 64;

impl TerminalHub {
    pub(super) fn run(&self, cwd: &str, commands: Receiver<Command>, stop: Arc<AtomicBool>) {
        let mut backend = PtyBackend::new(cwd, self.shell.clone());
        // Only the plugins some configured pane opted into are launched.
        let mut plugins = Plugins::start(cwd, &self.plugins, &self.startup);
        // What each pane's program has done to its terminal, so a client that
        // attaches later can be told rather than left to infer it from a replay.
        let mut modes = PaneModeTracker::default();
        // Why this is here at all: `hub_diag`.
        let mut clears = ClearWatch::default();

        while !stop.load(Ordering::Acquire) {
            let mut commands_since_resize = 0;
            while let Ok(command) = commands.try_recv() {
                if commands_since_resize == COMMANDS_BETWEEN_RESIZES {
                    for resize in self.take_pending_resizes() {
                        self.resize_pane(&mut backend, &mut modes, resize);
                    }
                    commands_since_resize = 0;
                }
                commands_since_resize += 1;
                match command {
                    Command::Create {
                        rows,
                        cols,
                        client,
                        command,
                    } => {
                        // Slots reserved for a claimed startup set count here.
                        if !self.has_free_slot() {
                            self.send_error_to(client, "terminal limit reached");
                            continue;
                        }
                        match backend.open_pane(rows, cols, command.as_deref()) {
                            // Unnamed: a pane a client asked for is that client's
                            // to name. No plugin association either — a shell a
                            // client opened is nobody's to drive but the person
                            // at it.
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
                    Command::ReloadPlugins { plugins: configs } => {
                        self.reload_hub_plugins(&mut backend, &mut plugins, &configs)
                    }
                    _ => {}
                }
            }

            // Resize is latest-value state, not a byte stream. Also processed
            // above after each command budget so a producer that continuously
            // refills the bounded queue cannot starve the final geometry.
            for resize in self.take_pending_resizes() {
                self.resize_pane(&mut backend, &mut modes, resize);
            }

            // Alternate-screen panes whose screen this tick's output has moved
            // on. Snapshotted once at the end rather than per chunk: a busy
            // program sends many small chunks, and serializing a grid per chunk
            // would be the most expensive thing on this path.
            let mut restless: Vec<PaneId> = Vec::new();
            for event in backend.drain_events() {
                match event {
                    BackendEvent::Output { pane, data } => {
                        plugins.pane_output(&backend, pane, &data);
                        // Before the lock: opening a pane's tracking emulator
                        // reads the shared state for its size.
                        let observed = modes.observe(pane, &data, || {
                            self.pane_size(pane)
                                .unwrap_or((DEFAULT_PANE_SIZE.rows, DEFAULT_PANE_SIZE.cols))
                        });
                        let alt = observed.modes.alt_screen;
                        // A chunk that moved the pane onto the alternate screen
                        // carries the switch and the first paint together, so its
                        // screen is taken in the same breath as the record —
                        // provided the chunk ends clean (`at_boundary`, below).
                        // Cut mid-sequence it is filed into `since` instead, and
                        // the tick's restless pass takes the screen once the
                        // stream closes; until then a connecting client replays
                        // `since` raw, landing pre-switch text on the wrong
                        // buffer for that moment — the paint that follows covers
                        // it, and the retry replaces it.
                        let screen = (alt && observed.alt_changed && modes.at_boundary(pane))
                            .then(|| modes.snapshot(pane))
                            .flatten();
                        let owed = self.record_and_broadcast(pane, data, observed, screen);
                        // Snapshots wait for a chunk that ends with its
                        // sequences closed (`at_boundary`): the snapshot is
                        // spliced into the recorded stream on replay, and a seam
                        // inside a sequence hands a reattaching client its tail
                        // as ordinary input. A crowded record retries with every
                        // next chunk; a desperate one has waited a whole extra
                        // ring, which no real sequence spans, so the records are
                        // bounded over a torn seam. Desperation overrides the
                        // sequence seam only — a grid missing a synchronized
                        // update's bytes (`screen_current`) must never be
                        // snapshotted, and needs no override: the update ends at
                        // the processor's own buffer cap if nothing else.
                        let crowded = owed > limits::MAX_TERMINAL_SCROLLBACK_BYTES;
                        let desperate = owed > 2 * limits::MAX_TERMINAL_SCROLLBACK_BYTES;
                        let ready =
                            modes.at_boundary(pane) || (desperate && modes.screen_current(pane));
                        if alt {
                            // Refreshed now rather than at the end of the tick: what
                            // a connecting client is handed on top of the screen has
                            // to stay bounded, and only a fresh screen bounds it.
                            if crowded && ready {
                                if let Some(screen) = modes.snapshot(pane) {
                                    self.store_screen(pane, screen);
                                }
                            } else if !restless.contains(&pane) {
                                restless.push(pane);
                            }
                        } else if crowded && ready {
                            // Not per tick like the alternate screen's: between
                            // snapshots the tail keeps the replay exact on its
                            // own, so this costs a serialization once per
                            // ring's worth of output.
                            if let Some(screen) = modes.snapshot(pane) {
                                self.store_normal_screen(pane, screen);
                            }
                        }
                    }
                    // Destroyed as well as forgotten: `PtyBackend` leaves pane
                    // removal to its caller, so a pane that ended on its own
                    // would otherwise keep its entry, its PTY master, and its
                    // child handle for the hub's whole life. The cap counts
                    // live panes, not those, so open-and-exit in a loop
                    // accumulated descriptors with nothing to stop it.
                    BackendEvent::Exited { pane } => {
                        modes.forget(pane);
                        clears.forget(pane);
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
                    | BackendEvent::Attention { .. }
                    | BackendEvent::Recovery { .. } => {
                        tracing::debug!("hub: unexpected session event from its own backend");
                    }
                }
            }

            // A program killed inside a synchronized update never closes it,
            // and the pane it leaves behind never produces enough to close it
            // at the processor's buffer cap either. Ended on the clock, so a
            // grid no byte will ever release stops holding the pane's modes
            // and every snapshot taken from it.
            for (pane, observed) in modes.settle_sync(Instant::now()) {
                let alt = observed.modes.alt_screen;
                self.store_settled(pane, observed);
                if alt && !restless.contains(&pane) {
                    restless.push(pane);
                }
            }

            // Once per tick, for every alternate-screen pane this tick moved: the
            // screen a connecting client is given, and the point at which what it
            // is owed on top of that screen goes back to nothing. A pane whose
            // last chunk ended mid-sequence keeps its old anchor — it is restless
            // again the moment it produces more output.
            for pane in restless {
                if modes.at_boundary(pane)
                    && let Some(screen) = modes.snapshot(pane)
                {
                    self.store_screen(pane, screen);
                }
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

            // A sizing owner that disconnected is held for a grace, so that
            // switching repositories — which closes one socket and opens
            // another — does not re-fit every pane there and back. Nothing runs
            // on its own to end that, so the tick does.
            self.settle_size_owner(Instant::now());
            thread::sleep(POLL_INTERVAL);
        }

        // Ahead of the panes: a plugin child is not one of `PtyBackend`'s panes,
        // so this is the only place it is ever reaped, and it must be told to
        // stop before its panes disappear beneath it.
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
        // terminals. The zoom goes with them — it names one of these panes.
        //
        // Announced rather than dropped in silence: `connect`'s guard against
        // replaying them cannot be airtight — a connection that took the state
        // lock first read `stop` before it was set, and has already been handed
        // every pane. This closes that window from the other side; the guard
        // keeps the common case cheap and this makes the outcome correct
        // either way.
        let mut state = self.state.lock().expect("terminal state poisoned");
        let gone: Vec<PaneId> = state.panes.iter().map(|p| p.id).collect();
        state.panes.clear();
        state.zoomed = None;
        for pane in gone {
            if let Ok(json) = serde_json::to_string(&ServerMessage::Exited { pane }) {
                broadcast_locked(&mut state.clients, TerminalFrame::Control(json));
            }
        }
    }
}
