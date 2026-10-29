//! Making a plugin's recovery visible to people, and cancellable by them.
//!
//! The recovery itself belongs to the plugin; what the hub owns is the hold on
//! an exited pane's slot (cancelling is the one place here that touches the
//! backend) and a cache of each pane's last report — a report is broadcast
//! once, so a client attaching later is replayed the cache (`connect`).

use super::TerminalHub;
use super::frame::{ServerMessage, TerminalFrame};
use super::hub_helpers::{RecoveryReport, broadcast_locked};
use super::hub_plugins::Plugins;
use crate::backend::{PaneId, PtyBackend, TerminalBackend};
use std::time::Instant;

/// End every standing report, for the hub's teardown. An `Exited` cannot stand
/// in for these — a report is *meant* to survive its pane's removal (that is
/// the relaunch hold) — so only `cancelled` ends one. Takes the already-held
/// state because teardown broadcasts everything under one hold.
pub(super) fn end_all_recoveries_locked(state: &mut super::hub_helpers::Shared) {
    let reported: Vec<PaneId> = state.recovery.keys().copied().collect();
    state.recovery.clear();
    for pane in reported {
        if let Ok(json) = serde_json::to_string(&ServerMessage::Recovery {
            pane,
            state: RECOVERY_CANCELLED.to_string(),
            detail: None,
            deadline_epoch: None,
            attempt: 0,
        }) {
            broadcast_locked(&mut state.clients, TerminalFrame::Control(json));
        }
    }
}

/// Whether `pane`'s process could be put back if it ended. A relaunch
/// reproduces the pane's original invocation, so a pane the host launched no
/// command in has nothing to reproduce — the guard refuses such a relaunch
/// outright, so the hold that exists solely to make one possible must not be
/// taken out for it either.
pub(super) fn is_relaunchable(backend: &PtyBackend, pane: PaneId) -> bool {
    backend
        .slot(pane)
        .is_some_and(|slot| slot.launch.command.is_some())
}

/// The `state` the hub sends when a pane's recovery is over without having
/// succeeded — cancelled by a person, or given up on when the hold ran out.
pub(crate) const RECOVERY_CANCELLED: &str = "cancelled";

/// The `state` a plugin reports when a pane is waiting on a person — the
/// bundled recovery plugin's label for an agent that finished or stalled. The
/// one plugin label the hub reads, and only to raise a [`Notice`] on entry.
///
/// [`Notice`]: crate::session::notice::Notice
pub(crate) const RECOVERY_NEEDS_ATTENTION: &str = "needs_attention";

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
        let mut shared = self.state.lock().expect("terminal state poisoned");
        // Entering the plugin's "needs attention" state is the one report that
        // reaches past this hub's clients; a repeat of it is not news.
        let newly_needs_attention = state == RECOVERY_NEEDS_ATTENTION
            && shared.recovery.get(&pane).map(|r| r.state.as_str()) != Some(state);
        // The cache a later client is replayed from (`connect`), kept under the
        // rule every connected client's map follows: `cancelled` removes.
        // Updated under the same hold as the broadcast so the two cannot
        // disagree about which side of this report a connection landed on.
        if state == RECOVERY_CANCELLED {
            shared.recovery.remove(&pane);
        } else {
            shared.recovery.insert(
                pane,
                RecoveryReport {
                    state: state.to_string(),
                    detail: detail.map(str::to_string),
                    deadline_epoch,
                    attempt,
                },
            );
        }
        broadcast_locked(&mut shared.clients, TerminalFrame::Control(json));
        drop(shared);
        if newly_needs_attention {
            self.publish_notice(pane, detail.map(str::to_string));
        }
    }

    /// Tell every client there is nothing pending for `pane` any more. Sent
    /// wherever a hold ends without leaving one behind — cancelled, expired,
    /// relaunched, or closed for good.
    pub(super) fn end_recovery(&self, pane: PaneId) {
        self.broadcast_recovery(pane, RECOVERY_CANCELLED, None, None, 0);
    }

    /// [`end_recovery`](Self::end_recovery), but only when a report is standing
    /// (`Shared.recovery`). For the paths that merely *might* trail one — a
    /// pane whose watcher a reload dropped, an unwatched pane closing — where
    /// an unconditional broadcast would announce the end of a recovery nobody
    /// was ever shown. Check and broadcast are two lock holds, which is safe
    /// because the worker thread is the only mutator of the cache.
    pub(super) fn end_recovery_if_reported(&self, pane: PaneId) {
        let standing = self
            .state
            .lock()
            .expect("terminal state poisoned")
            .recovery
            .contains_key(&pane);
        if standing {
            self.end_recovery(pane);
        }
    }

    /// A pane's process ended. For a pane no plugin watches this is the
    /// long-standing path: destroy it and tell everyone. For a watched one the
    /// slot survives — the process alone is let go and the slot is held until
    /// the plugin acts or the window closes. Unless there is nothing to put
    /// back: see [`is_relaunchable`].
    pub(super) fn pane_exited(
        &self,
        backend: &mut PtyBackend,
        plugins: &mut Plugins,
        pane: PaneId,
    ) {
        if plugins.owner(pane).is_none() {
            // An unwatched pane can still trail a report from a watcher it
            // lost (a plugin a reload dropped); its exit is where that ends.
            self.end_recovery_if_reported(pane);
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
            // Nowhere to put a replacement, or nothing to put there.
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
