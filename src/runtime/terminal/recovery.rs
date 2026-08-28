//! What a pane's plugin last said about getting it running again.
//!
//! Held per pane and nowhere near the emulators: this is metadata the session
//! reports, not screen content, so it never touches a grid and a pane without a
//! report costs one absent map entry.

use crate::backend::PaneId;

use super::TerminalState;

/// The `state` a session sends once a pane's recovery is over without having
/// succeeded. Mirrors the hub's own constant; the wire is the contract between
/// them, so the value is asserted rather than shared.
pub const RECOVERY_CANCELLED: &str = "cancelled";

/// One pane's latest recovery report.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PaneRecovery {
    /// The plugin's own short label, e.g. `waiting_for_reset`.
    pub state: String,
    /// A short human line, when the plugin gave one.
    pub detail: Option<String>,
    /// When the wait ends, in unix epoch seconds. `None` when the plugin is not
    /// waiting on a clock — the renderer then shows no time at all rather than
    /// inventing one.
    pub deadline_epoch: Option<i64>,
    pub attempt: u32,
}

impl TerminalState {
    /// Record what the session reports about `pane`.
    ///
    /// A `cancelled` report *clears* the entry instead of storing it: there is
    /// nothing left to wait for, and keeping the label would leave a badge on a
    /// pane whose recovery is over.
    pub(super) fn apply_recovery(
        &mut self,
        pane: PaneId,
        state: String,
        detail: Option<String>,
        deadline_epoch: Option<i64>,
        attempt: u32,
    ) {
        if state == RECOVERY_CANCELLED {
            self.recovery.remove(&pane);
            return;
        }
        self.recovery.insert(
            pane,
            PaneRecovery {
                state,
                detail,
                deadline_epoch,
                attempt,
            },
        );
    }

    /// What `pane`'s plugin last reported, if anything.
    pub fn recovery_for(&self, pane: PaneId) -> Option<&PaneRecovery> {
        self.recovery.get(&pane)
    }

    /// The one report a person is looking at, and the one the cancel key acts
    /// on: the focused pane's own report first, failing that a report for a
    /// pane this client no longer lists (its process ended while its slot is
    /// held for a relaunch). That pane cannot be focused, and it is exactly
    /// the one someone would want to release. Lowest id wins so the display
    /// and the key can never disagree about which one that is.
    pub fn recovery_focus(&self) -> Option<(PaneId, &PaneRecovery)> {
        if let Some(pane) = self.active_pane_id()
            && let Some(report) = self.recovery.get(&pane)
        {
            return Some((pane, report));
        }
        self.recovery
            .iter()
            .filter(|(pane, _)| !self.panes.iter().any(|p| p.id == **pane))
            .min_by_key(|(pane, _)| **pane)
            .map(|(pane, report)| (*pane, report))
    }

    /// Whether the cancel key would do anything. Single source for the key gate
    /// and the hint row, so a hint can never advertise a no-op.
    pub fn can_cancel_recovery(&self) -> bool {
        self.recovery_focus().is_some()
    }

    /// Ask the session to give up on the recovery a person is looking at.
    /// Nothing is cleared here: the entry goes when the session broadcasts
    /// `cancelled` (which tells every other client too) — assuming it locally
    /// would hide a cancellation the session refused.
    pub fn cancel_recovery(&mut self) {
        let Some((pane, _)) = self.recovery_focus() else {
            return;
        };
        if let Some(backend) = &mut self.backend {
            backend.cancel_recovery(pane);
        }
    }
}
