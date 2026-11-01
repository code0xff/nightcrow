use super::TerminalHub;
use super::frame::{ServerMessage, TerminalFrame};
use super::hub_diag::ClearWatch;
use super::hub_helpers::{Command, broadcast_locked, resize_due_before_command};
use super::hub_modes::PaneModeTracker;
use super::hub_plugins::Plugins;
use crate::backend::{BackendEvent, PaneId, PtyBackend, TerminalBackend};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::Receiver;
use std::time::{Duration, Instant};

/// The fixed wait the worker used before it could be woken. Kept so the ignored
/// measurement can reproduce that cadence and count the passes it cost.
const POLL_INTERVAL: Duration = Duration::from_millis(8);

/// Longest the worker waits with nothing to do. Only bounds the cost of a signal
/// a producer failed to send; every pass that has work is owed a signal or a
/// deadline, and both end the wait sooner.
const IDLE_WAIT_CAP: Duration = Duration::from_secs(1);

impl TerminalHub {
    pub(super) fn run(&self, cwd: &str, commands: Receiver<Command>, stop: Arc<AtomicBool>) {
        let mut backend = PtyBackend::new(cwd, self.shell.clone());
        // The PTY reader and waiter threads report queued events rather than
        // leaving this loop to poll for them.
        backend.set_waker(Arc::clone(&self.wake));
        // Only the plugins some configured pane opted into are launched.
        let mut plugins = Plugins::start(
            cwd,
            &self.plugins,
            &self.startup,
            Some(Arc::clone(&self.wake)),
        );
        // What each pane's program has done to its terminal, so a client that
        // attaches later can be told rather than left to infer it from a replay.
        let mut modes = PaneModeTracker::default();
        // Why this is here at all: `hub_diag`.
        let mut clears = ClearWatch::default();

        while !stop.load(Ordering::Acquire) {
            #[cfg(test)]
            self.probe.passes.fetch_add(1, Ordering::Relaxed);
            for pane in self.take_pending_closes() {
                if !self.pane_is_live(pane) {
                    continue;
                }
                if plugins.owner(pane).is_some() {
                    plugins.pane_closed(&backend, pane);
                    plugins.forget(&backend, pane);
                    backend.retire_slot(pane);
                    self.end_recovery(pane);
                } else {
                    // A watcher a reload dropped can leave a report (`pane_exited`).
                    self.end_recovery_if_reported(pane);
                }
                modes.forget(pane);
                clears.forget(pane);
                backend.destroy_pane(pane);
                self.remove_pane_and_announce(pane);
            }
            let mut commands_since_resize = 0;
            while let Ok(command) = commands.try_recv() {
                if resize_due_before_command(&mut commands_since_resize) {
                    for resize in self.take_pending_resizes() {
                        self.resize_pane(&mut backend, &mut modes, resize);
                    }
                }
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
                            // Unnamed and unwatched: a shell a client opened is
                            // that client's to name and nobody else's to drive.
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
                        // Human input cancels plugin plans before the bytes land,
                        // so its cancellation cannot be overtaken by new output.
                        plugins.user_input(&backend, pane);
                        if let Err(error) = backend.send_input(pane, &data) {
                            tracing::warn!(pane, client, %error, "viewer: could not queue terminal input");
                            self.send_error_to(client, "terminal input could not be sent");
                        }
                    }
                    Command::Reorder { order } => self.reorder_panes(order),
                    // Not gated on the pane being live: a pending recovery's
                    // process has already ended and left the visible list.
                    Command::CancelRecovery { pane } => {
                        self.cancel_recovery(&mut backend, &mut plugins, pane)
                    }
                    Command::ReloadPlugins { plugins: configs } => {
                        self.reload_hub_plugins(&mut backend, &mut plugins, &configs)
                    }
                    Command::Notify {
                        token,
                        message,
                        reply,
                    } => self.notify_by_token(&backend, &token, message, &reply),
                    _ => {}
                }
            }

            // Resize is latest-value state, not a byte stream. The interleave
            // above also observes it after each command budget so a producer
            // that continuously refills the queue cannot starve final geometry.
            for resize in self.take_pending_resizes() {
                self.resize_pane(&mut backend, &mut modes, resize);
            }

            // Alternate-screen panes whose screen this tick's output has moved
            // on. Snapshotted once at the end rather than per chunk: a busy
            // program sends many small chunks, and serializing a grid per chunk
            // would be the most expensive thing on this path.
            let mut restless: Vec<PaneId> = Vec::new();
            // One clock for the whole tick: the chunk path, the synchronized
            // update that ends on the clock, and the paced pass all read the
            // same moment, so a screen stored by one is not immediately due
            // to the other.
            let now = Instant::now();
            for event in backend.drain_events() {
                match event {
                    BackendEvent::Output { pane, data } => {
                        plugins.pane_output(&backend, pane, &data);
                        if self.record_pane_output(pane, data, &mut modes, now)
                            && !restless.contains(&pane)
                        {
                            restless.push(pane);
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
                    | BackendEvent::Recovery { .. }
                    | BackendEvent::ReplayOutput { .. }
                    | BackendEvent::ReplayComplete => {
                        tracing::debug!("hub: unexpected session event from its own backend");
                    }
                }
            }

            // A program killed inside a synchronized update never closes it,
            // and the pane it leaves behind never produces enough to close it
            // at the processor's buffer cap either. Ended on the clock, so a
            // grid no byte will ever release stops holding the pane's modes
            // and every snapshot taken from it.
            for (pane, observed) in modes.settle_sync(now) {
                let alt = observed.modes.alt_screen;
                self.store_settled(pane, observed);
                if alt && !restless.contains(&pane) {
                    restless.push(pane);
                }
            }

            // Once per tick, for every alternate-screen pane this tick moved and
            // for every pane that still owes bytes after falling idle: the screen
            // a connecting client is given, and the point at which what it is owed
            // on top of that screen goes back to nothing. Paced rather than taken
            // every tick — see `PaneModeTracker::settle_restless` — so a program
            // that paints several times per tick costs one serialization per
            // window instead of one per tick, while a quiet pane still gets a
            // trailing snapshot one window after its last write. A pane whose
            // last chunk ended mid-sequence keeps its old anchor — it is restless
            // again the moment it produces more output.
            for (pane, screen) in modes.settle_restless(&restless, now) {
                self.store_screen(pane, screen);
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
            // on its own to end that, so the pass does.
            self.settle_size_owner(Instant::now());
            // A pass that left work undone — a command a full queue refused, a
            // plugin that filled its per-tick budget — has already been signalled
            // by whoever produced it, so the flag is what makes that pass follow
            // this one. Everything else is on a clock, and `next_deadline` names
            // the earliest of them.
            #[cfg(test)]
            let legacy_polling = self.probe.legacy_polling.load(Ordering::Acquire);
            #[cfg(not(test))]
            let legacy_polling = false;
            let cap = if legacy_polling {
                POLL_INTERVAL
            } else {
                IDLE_WAIT_CAP
            };
            let deadline = if legacy_polling {
                None
            } else {
                self.next_deadline(&backend, &modes, &plugins, Instant::now())
            };
            self.wake.wait(deadline, cap);
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
        // The standing reports go the way the panes do, and for the same
        // reason as the `Exited`s above: the sockets outlive the hub, and a
        // client left parked on one would keep an orphan chip whose cancel
        // nothing answers.
        super::hub_recovery::end_all_recoveries_locked(&mut state);
        #[cfg(test)]
        self.probe.done.store(true, Ordering::Release);
    }
}
