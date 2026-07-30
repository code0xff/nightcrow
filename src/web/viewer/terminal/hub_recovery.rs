//! Making a plugin's recovery visible to people, and cancellable by them.
//!
//! The hub keeps no recovery state of its own: a report is broadcast as it
//! arrives and forgotten. What the hub *does* own is the hold on an exited
//! pane's slot, and that is the thing a person can take away — so cancelling is
//! the one place here that touches the backend.

use super::TerminalHub;
use super::frame::{ServerMessage, TerminalFrame};
use super::hub_helpers::broadcast_locked;
use super::hub_plugins::Plugins;
use crate::backend::{PaneId, PtyBackend, TerminalBackend};
use std::time::Instant;

/// Whether `pane`'s process could be put back if it ended.
///
/// A relaunch reproduces the pane's original invocation, so a pane the host
/// launched no command in has nothing to reproduce — which is exactly the pane a
/// plugin is given when its occupant asks to be watched. The guard refuses such a
/// relaunch outright, so the hold that exists solely to make one possible must not
/// be taken out for it either: that hold lasts days, and it would be days spent
/// keeping a shell's slot alive for a request that can never be granted.
pub(super) fn is_relaunchable(backend: &PtyBackend, pane: PaneId) -> bool {
    backend
        .slot(pane)
        .is_some_and(|slot| slot.launch.command.is_some())
}

/// The `state` the hub itself sends when a pane's recovery is over without
/// having succeeded — cancelled by a person, or given up on when the hold ran
/// out. Every client reads it as "stop showing a deadline for this pane".
pub(crate) const RECOVERY_CANCELLED: &str = "cancelled";

impl TerminalHub {
    /// Tell every client the latest word on a pane's recovery.
    pub(super) fn broadcast_recovery(
        &self,
        pane: PaneId,
        state: &str,
        detail: Option<&str>,
        deadline_epoch: Option<i64>,
        attempt: u32,
    ) {
        let Ok(json) = serde_json::to_string(&ServerMessage::Recovery {
            pane,
            state: state.to_string(),
            detail: detail.map(str::to_string),
            deadline_epoch,
            attempt,
        }) else {
            return;
        };
        // Serialized before the lock and broadcast under it, exactly as the pane
        // announcements are: a client either connects before this frame or after,
        // never into the middle of it.
        let mut state = self.state.lock().expect("terminal state poisoned");
        broadcast_locked(&mut state.clients, TerminalFrame::Control(json));
    }

    /// Tell every client there is nothing pending for `pane` any more.
    ///
    /// Sent wherever a hold ends without leaving one behind — cancelled, expired,
    /// relaunched, or closed for good. Without it a client keeps the last report
    /// it saw, and a deadline that has already come and gone stays on screen.
    pub(super) fn end_recovery(&self, pane: PaneId) {
        self.broadcast_recovery(pane, RECOVERY_CANCELLED, None, None, 0);
    }

    /// A pane's process ended.
    ///
    /// For a pane no plugin watches this is the long-standing path: destroy it
    /// and tell everyone. For a watched one the slot has to survive, because its
    /// token is the only thing a relaunch can reuse — so the process alone is let
    /// go and the slot is held until the plugin acts or the window closes. Unless
    /// there is nothing to put back: see [`is_relaunchable`].
    pub(super) fn pane_exited(
        &self,
        backend: &mut PtyBackend,
        plugins: &mut Plugins,
        pane: PaneId,
    ) {
        if plugins.owner(pane).is_none() {
            backend.destroy_pane(pane);
            self.remove_pane_and_announce(pane);
            return;
        }
        // Where it sat, read before the removal below takes it out of the order.
        match self.pane_spot(pane) {
            Some(spot) if is_relaunchable(backend, pane) => {
                backend.release_process(pane);
                plugins.hold_for_relaunch(pane, spot, Instant::now());
                plugins.pane_exited(backend, pane);
            }
            // Nowhere to put a replacement, or nothing to put there: either way
            // there is no reason to keep the slot alive for one.
            _ => {
                plugins.pane_closed(backend, pane);
                plugins.forget(backend, pane);
                backend.destroy_pane(pane);
                self.end_recovery(pane);
            }
        }
        // Clients see the truth either way: the process is gone, and a relaunch
        // arrives as a new pane rather than as this one coming back.
        self.remove_pane_and_announce(pane);
    }

    /// A person has given up on `pane`'s recovery.
    ///
    /// Taking the hold is what makes this a cancellation rather than a no-op: the
    /// hold *is* the pending recovery, so a pane without one has nothing to
    /// invalidate and is left alone. When there is one, the plugin is told the
    /// slot is going while it can still be named, and only then is it retired —
    /// `forget` reads the slot's token to drop the plugin's spent budget, so it
    /// has to run before `retire_slot` takes the slot away.
    pub(super) fn cancel_recovery(
        &self,
        backend: &mut PtyBackend,
        plugins: &mut Plugins,
        pane: PaneId,
    ) {
        if plugins.claim_pending(pane).is_none() {
            tracing::debug!(pane, "viewer: cancel with no recovery pending; ignored");
            return;
        }
        tracing::info!(pane, "viewer: a client cancelled a pane's recovery");
        plugins.pane_closed(backend, pane);
        plugins.forget(backend, pane);
        backend.retire_slot(pane);
        self.end_recovery(pane);
    }
}
