//! Carrying out what a plugin was allowed to do.
//!
//! Everything here runs on the worker thread and acts only on an [`Approved`]
//! value. There is no path from a [`PluginCommand`](crate::plugin::protocol::PluginCommand)
//! to a pane that does not pass through [`Plugins::judge`] first, and a
//! [`Refused`] is logged and dropped — never retried, never acted on.

use super::TerminalHub;
use super::frame::{ServerMessage, TerminalFrame};
use super::hub_helpers::broadcast_locked;
use super::hub_plugins::Plugins;
use super::hub_plugins_slots::PaneSpot;
use crate::backend::{PaneId, PtyBackend, TerminalBackend};
use crate::plugin::protocol::LogLevel;
use crate::plugin::{Approved, Refused};
use std::time::Instant;

impl TerminalHub {
    /// Take everything the plugins have asked for this tick, judge it, and do
    /// whatever survived.
    pub(super) fn dispatch_plugin_commands(
        &self,
        backend: &mut PtyBackend,
        plugins: &mut Plugins,
        now: Instant,
    ) {
        for (plugin, command) in plugins.take_commands() {
            let verdict = plugins.judge(&plugin, command, backend, now);
            match verdict {
                // Straight to the PTY rather than back through
                // `Command::Input`: that queue is the human's, and a plugin's
                // own input arriving on it would raise `UserInput` and cancel
                // the very recovery this input is part of.
                Ok(Approved::SendInput { pane, data }) => {
                    if let Err(error) = backend.send_input(pane, &data) {
                        tracing::warn!(
                            plugin = %plugin,
                            pane,
                            %error,
                            "viewer: plugin input could not be written"
                        );
                    }
                }
                Ok(Approved::Relaunch {
                    pane,
                    resume_args,
                    command_line,
                }) => self.relaunch_for_plugin(
                    backend,
                    plugins,
                    &plugin,
                    pane,
                    &resume_args,
                    &command_line,
                ),
                // Observability, and now a client-visible one. Still logged
                // whole: the log is the record of what a plugin claimed, and a
                // client that was not connected saw none of it.
                Ok(Approved::Status {
                    pane,
                    state,
                    detail,
                    deadline_epoch,
                    attempt,
                }) => {
                    tracing::info!(
                        plugin = %plugin,
                        pane,
                        state = %state,
                        detail = ?detail,
                        deadline_epoch = ?deadline_epoch,
                        attempt,
                        "viewer: plugin reports a pane's state"
                    );
                    self.broadcast_recovery(
                        pane,
                        &state,
                        detail.as_deref(),
                        deadline_epoch,
                        attempt,
                    );
                }
                Ok(Approved::WatchPane { pane }) => {
                    self.watch_pane_for_plugin(backend, plugins, &plugin, pane)
                }
                Ok(Approved::Log { level, message }) => log_plugin_line(&plugin, level, &message),
                Err(refused) => log_refusal(&plugin, &refused),
            }
        }
    }

    /// Hand `pane` to `plugin` and announce it, so the plugin can start from the
    /// same `PaneOpened` a configured pane begins with.
    ///
    /// Announcing is the whole point of the request: the association alone tells
    /// the plugin nothing, and only a `PaneOpened` carries the generation every
    /// later command has to name. The pane's history is not replayed — output
    /// events carry fresh bytes only — so a plugin taken on mid-session sees the
    /// pane from here forward, which is exactly what the signal that brought it
    /// here is about.
    fn watch_pane_for_plugin(
        &self,
        backend: &PtyBackend,
        plugins: &mut Plugins,
        plugin: &str,
        pane: PaneId,
    ) {
        // The pane's name as the clients know it, so a plugin sees the same title
        // whichever way it was given the pane. A client-opened pane has none.
        let title = self.pane_spot(pane).and_then(|spot| spot.title);
        if !plugins.adopt(pane, plugin) {
            // Only reachable if the plugin's host died between the command being
            // queued and this tick, since a dead host produces no commands.
            tracing::debug!(
                plugin = %plugin,
                pane,
                "viewer: nothing to hand a pane to; the plugin's host is gone"
            );
            return;
        }
        tracing::info!(
            plugin = %plugin,
            pane,
            "viewer: a plugin was given a pane by the token something inside it quoted"
        );
        plugins.pane_opened(backend, pane, title.as_deref());
    }

    /// Put a process back into the slot a plugin was holding.
    ///
    /// Only for a pane the hub is actually holding: that hold is the proof the
    /// pane exited and is still inside its window, and without it the slot has
    /// either been retired or never belonged to this flow at all.
    fn relaunch_for_plugin(
        &self,
        backend: &mut PtyBackend,
        plugins: &mut Plugins,
        plugin: &str,
        pane: PaneId,
        resume_args: &[String],
        command_line: &str,
    ) {
        let Some(held) = plugins.claim_pending(pane) else {
            tracing::debug!(
                plugin = %plugin,
                pane,
                "viewer: relaunch for a pane with no hold; ignored"
            );
            return;
        };
        // The cap still binds. The pane's own slot came free when its process
        // ended, but a hold can be open for hours and a client may have taken
        // that slot in the meantime — and the ceiling counts real processes, not
        // who is entitled to one. Left held so the plugin can try again while its
        // window lasts, rather than losing the recovery to a full grid.
        if !self.has_free_slot() {
            tracing::warn!(
                plugin = %plugin,
                pane,
                "viewer: no terminal slot free for a relaunch; the pane keeps its hold"
            );
            plugins.restore_pending(pane, held);
            return;
        }
        let (rows, cols, index) = (held.spot.rows, held.spot.cols, held.spot.index);
        let title = held.spot.title.clone();
        let allowed = plugins
            .allowed_flags
            .get(plugin)
            .cloned()
            .unwrap_or_default();

        match backend.relaunch_pane(pane, rows, cols, resume_args, &allowed) {
            Ok(replacement) => {
                // `relaunch_pane` composes the line itself from the same slot,
                // args and flags the guard used, so the approved text is the
                // text that runs; logging it records exactly that without
                // giving the command line a second source of truth.
                tracing::info!(
                    plugin = %plugin,
                    pane,
                    replacement,
                    command_line = %command_line,
                    "viewer: relaunched a pane for its plugin"
                );
                self.register_pane(replacement, rows, cols, None, title.clone());
                self.restore_pane_index(replacement, index);
                plugins.take_over(pane, replacement);
                plugins.pane_opened(backend, replacement, title.as_deref());
                // The old pane id is spent, so any report a client is still
                // showing for it is now about nothing. The plugin's next report
                // arrives under the replacement's id.
                self.end_recovery(pane);
            }
            // Held rather than retried. The window is what bounds this, and a
            // retry loop here would spend the plugin's whole relaunch budget
            // inside one 8 ms tick.
            Err(error) => {
                tracing::warn!(
                    plugin = %plugin,
                    pane,
                    %error,
                    "viewer: relaunch failed; the pane keeps its hold"
                );
                plugins.restore_pending(pane, held);
            }
        }
    }

    /// Put `pane` back at `index` in the client-visible order and tell every
    /// client the result.
    ///
    /// The order *is* `Shared::panes`, so this is how a relaunched pane keeps
    /// its predecessor's place without a new wire message: clients already
    /// apply `Reordered`.
    fn restore_pane_index(&self, pane: PaneId, index: usize) {
        let mut state = self.state.lock().expect("terminal state poisoned");
        let Some(from) = state.panes.iter().position(|p| p.id == pane) else {
            return;
        };
        // Clamped, because panes can have closed while the hold was open and an
        // index past the end would panic `insert`.
        let to = index.min(state.panes.len().saturating_sub(1));
        if from == to {
            return;
        }
        let entry = state.panes.remove(from);
        state.panes.insert(to, entry);
        let order: Vec<PaneId> = state.panes.iter().map(|p| p.id).collect();
        if let Ok(json) = serde_json::to_string(&ServerMessage::Reordered { order }) {
            broadcast_locked(&mut state.clients, TerminalFrame::Control(json));
        }
    }

    /// Where `pane` sits and what it looks like, captured before it is removed
    /// so its replacement can be put back in the same place under the same name.
    pub(super) fn pane_spot(&self, pane: PaneId) -> Option<PaneSpot> {
        let state = self.state.lock().expect("terminal state poisoned");
        let index = state.panes.iter().position(|p| p.id == pane)?;
        Some(PaneSpot::of(index, &state.panes[index]))
    }
}

/// A plugin's own log line, attributed to it so it cannot be mistaken for one
/// of nightcrow's.
fn log_plugin_line(plugin: &str, level: LogLevel, message: &str) {
    match level {
        LogLevel::Error => tracing::error!(plugin = %plugin, "{message}"),
        LogLevel::Warn => tracing::warn!(plugin = %plugin, "{message}"),
        LogLevel::Info => tracing::info!(plugin = %plugin, "{message}"),
        LogLevel::Debug => tracing::debug!(plugin = %plugin, "{message}"),
    }
}

/// Log a refusal at the level that says whether anyone should look into it.
///
/// A plugin decides asynchronously, so being late is ordinary traffic rather
/// than a fault: the pane moved on, is not quiet yet, or was claimed by another
/// plugin first. The rest mean the plugin asked for something it was never
/// allowed — a pane that is not its, an oversized or control-laden payload, a
/// flag the config does not list, a bare shell it wanted to relaunch, or more
/// attempts than the budget allows — and that is worth an operator's attention.
/// Matched exhaustively on purpose, so a new refusal has to be classified rather
/// than defaulting to silence.
fn log_refusal(plugin: &str, refused: &Refused) {
    let ordinary = match refused {
        Refused::UnknownPane { .. }
        | Refused::StaleGeneration { .. }
        | Refused::PaneNotRunning { .. }
        | Refused::PaneBusy { .. }
        | Refused::PaneStillRunning { .. }
        | Refused::PaneWatchedByAnother { .. } => true,
        Refused::NotOptedIn { .. }
        | Refused::InputTooLarge { .. }
        | Refused::ControlCharacter { .. }
        | Refused::NoLaunchCommand { .. }
        | Refused::ResumeArgsRejected { .. }
        | Refused::WatchNotAllowed { .. }
        | Refused::RateLimited { .. } => false,
    };
    if ordinary {
        tracing::debug!(plugin = %plugin, "viewer: plugin command refused: {refused}");
    } else {
        tracing::warn!(plugin = %plugin, "viewer: plugin command refused: {refused}");
    }
}
