//! Which pane belongs to which plugin, and how long an exited one's slot is kept.
//!
//! Split from the host lifecycle beside it because the two are bounded by
//! different things. Whether a plugin child runs is decided by the config; whether
//! a pane is still reachable is decided by that pane's own process and by the
//! window a relaunch has left, which is what everything here measures.

use super::hub_helpers::PaneState;
use super::hub_plugins::Plugins;
use crate::backend::{PaneId, PtyBackend};
use std::time::{Duration, Instant};

/// How long an exited pane's slot is kept so a relaunch can still reuse its
/// token.
///
/// This is a backstop against a plugin that died or lost interest, so it has to
/// outlast every wait a plugin may legitimately be in the middle of. Providers
/// quote windows in hours *and* in days — a weekly quota is a real case — so a
/// value picked around the five-hour window would silently throw the pane's
/// identity away days before the wait paid off, and the relaunch it was being
/// kept for would fail. Nine days clears the longest window a bundled plugin
/// will wait out (`nightcrow-recovery`'s own clamp is eight days) with slack for
/// a reset that lands late.
///
/// Holding it that long is cheap on purpose: a token, a generation and a command
/// string. The process, its fds and its threads were let go the moment it exited
/// (see [`PtyBackend::release_process`]), and closing the pane or stopping the
/// session retires the slot immediately either way.
pub(super) const PENDING_RELAUNCH_TTL: Duration = Duration::from_secs(9 * 24 * 60 * 60);

/// Where a pane sat and what it looked like, captured before it is removed.
pub(super) struct PaneSpot {
    /// Its position in the client-visible order, so a relaunch lands back where
    /// the operator left it instead of at the end of the row.
    pub(super) index: usize,
    pub(super) rows: u16,
    pub(super) cols: u16,
    pub(super) title: Option<String>,
}

impl PaneSpot {
    pub(super) fn of(index: usize, pane: &PaneState) -> Self {
        Self {
            index,
            rows: pane.rows,
            cols: pane.cols,
            title: pane.title.clone(),
        }
    }
}

/// A pane whose process exited while a plugin was watching it, held so that
/// plugin still has something to relaunch.
pub(super) struct Pending {
    pub(super) spot: PaneSpot,
    /// When the slot is given up on. See [`PENDING_RELAUNCH_TTL`].
    deadline: Instant,
}

impl Plugins {
    /// Hold `pane`'s slot open for a relaunch. Nothing is held for a pane no
    /// plugin watches — that pane's slot is gone by the time this is reached.
    pub(super) fn hold_for_relaunch(&mut self, pane: PaneId, spot: PaneSpot, now: Instant) {
        if !self.owners.contains_key(&pane) {
            return;
        }
        self.idle_announced.remove(&pane);
        self.pending.insert(
            pane,
            Pending {
                spot,
                deadline: now + PENDING_RELAUNCH_TTL,
            },
        );
    }

    /// Take the hold on `pane`, if it is still within its window.
    pub(super) fn claim_pending(&mut self, pane: PaneId) -> Option<Pending> {
        self.pending.remove(&pane)
    }

    /// Put a hold back after a relaunch attempt failed, so the pane keeps its
    /// remaining window instead of being retired by a single bad try.
    pub(super) fn restore_pending(&mut self, pane: PaneId, pending: Pending) {
        self.pending.insert(pane, pending);
    }

    /// Move a pane's association onto the process that replaced it.
    ///
    /// The spent budget is deliberately left alone. It is keyed by the slot's
    /// token, which a relaunch preserves, and that is the only thing bounding a
    /// plugin that answers every exit with another relaunch — clearing it here
    /// would hand out a fresh allowance on every attempt and the ceiling would
    /// never be reached.
    pub(super) fn take_over(&mut self, old: PaneId, new: PaneId) {
        if let Some(plugin) = self.owners.remove(&old) {
            self.owners.insert(new, plugin);
        }
        self.idle_announced.remove(&old);
    }

    /// Forget `pane` entirely. The caller still has to retire its slot.
    ///
    /// Takes the backend because the budget is keyed by the slot's token, so it
    /// has to be read before the slot is retired.
    pub(super) fn forget(&mut self, backend: &PtyBackend, pane: PaneId) {
        // The pane itself is going, so what it had opted into goes with it. A
        // plugin being stopped by a reload takes the narrower path
        // (`release_pane`), which leaves the opt-in for a later reload to honour.
        self.intended.remove(&pane);
        self.owners.remove(&pane);
        self.pending.remove(&pane);
        self.idle_announced.remove(&pane);
        if let Some(slot) = backend.slot(pane) {
            self.guard.cancel(&slot.identity.token.clone());
        }
    }

    /// Retire the slots nobody relaunched in time, reporting which panes those
    /// were so the caller can tell the clients still showing their deadlines.
    pub(super) fn expire_pending(&mut self, backend: &mut PtyBackend, now: Instant) -> Vec<PaneId> {
        let expired: Vec<PaneId> = self
            .pending
            .iter()
            .filter(|(_, held)| now >= held.deadline)
            .map(|(pane, _)| *pane)
            .collect();
        for pane in &expired {
            tracing::info!(
                pane,
                "viewer: no relaunch within the window; retiring the pane's slot"
            );
            self.forget(backend, *pane);
            backend.retire_slot(*pane);
        }
        expired
    }
}
