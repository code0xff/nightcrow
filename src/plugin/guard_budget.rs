//! How much a plugin may actually do to a pane, per pane, per window.
//!
//! The point is that a plugin's attempts are bounded and its failures visible:
//! a recovery flow that is not working must stop trying rather than type into a
//! pane forever.
//!
//! Counted per *slot*, keyed by [`PaneToken`], not per `PaneId`. A relaunch
//! always mints a new id, so an id-keyed budget would hand a fresh allowance to
//! every relaunch — a command that exits at once plus a plugin that relaunches
//! on every exit would then loop with nothing to stop it. The token is what
//! survives a relaunch, so it is what the ceiling has to be attached to.

use crate::backend::PaneToken;
use std::collections::HashMap;
use std::time::{Duration, Instant};

/// Window over which a pane's actions are counted.
pub const DEFAULT_RATE_WINDOW: Duration = Duration::from_secs(60);

/// Approved inputs per pane per window by default.
///
/// A recovery exchange is a prompt and perhaps a confirmation. More than a
/// handful in a minute is a loop, not a conversation.
pub const DEFAULT_MAX_SENDS: u32 = 5;

/// Approved relaunches per slot per window by default. Fewer than sends: a
/// relaunch that did not take is unlikely to take on the fourth try either.
pub const DEFAULT_MAX_RELAUNCHES: u32 = 3;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RateLimits {
    pub max_sends_per_window: u32,
    pub max_relaunches_per_window: u32,
    pub window: Duration,
}

impl Default for RateLimits {
    fn default() -> Self {
        Self {
            max_sends_per_window: DEFAULT_MAX_SENDS,
            max_relaunches_per_window: DEFAULT_MAX_RELAUNCHES,
            window: DEFAULT_RATE_WINDOW,
        }
    }
}

/// Which counter an action draws on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RateAction {
    SendInput,
    Relaunch,
}

impl RateAction {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::SendInput => "send_input",
            Self::Relaunch => "relaunch",
        }
    }
}

#[derive(Debug, Default)]
struct PaneBudget {
    sends: Vec<Instant>,
    relaunches: Vec<Instant>,
}

/// When each slot's approved actions happened, pruned as it is read.
#[derive(Debug, Default)]
pub(super) struct Budgets(HashMap<PaneToken, PaneBudget>);

impl Budgets {
    /// Charge one `action` against the slot's budget, if there is room.
    ///
    /// Nothing is recorded when there is not, so a refusal here costs the slot
    /// nothing beyond the refusal itself. Each list is capped at its limit by
    /// construction, so this stays O(limit).
    pub(super) fn try_spend(
        &mut self,
        token: &PaneToken,
        action: RateAction,
        limits: &RateLimits,
        now: Instant,
    ) -> bool {
        let budget = self.0.entry(token.clone()).or_default();
        let (stamps, max) = match action {
            RateAction::SendInput => (&mut budget.sends, limits.max_sends_per_window),
            RateAction::Relaunch => (&mut budget.relaunches, limits.max_relaunches_per_window),
        };
        stamps.retain(|at| now.saturating_duration_since(*at) < limits.window);
        if stamps.len() as u32 >= max {
            return false;
        }
        stamps.push(now);
        true
    }

    pub(super) fn clear(&mut self, token: &PaneToken) {
        self.0.remove(token);
    }

    /// How much of `action`'s budget the slot has spent inside the window. Lets
    /// a test assert on the budget itself rather than on the next refusal.
    #[cfg(test)]
    pub(super) fn spent(
        &mut self,
        token: &PaneToken,
        action: RateAction,
        limits: &RateLimits,
        now: Instant,
    ) -> u32 {
        let Some(budget) = self.0.get_mut(token) else {
            return 0;
        };
        let stamps = match action {
            RateAction::SendInput => &mut budget.sends,
            RateAction::Relaunch => &mut budget.relaunches,
        };
        stamps.retain(|at| now.saturating_duration_since(*at) < limits.window);
        stamps.len() as u32
    }
}
