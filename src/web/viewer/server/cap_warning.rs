//! How often the viewer may say that its connection cap is full.
//!
//! The cap is a real ceiling, and hitting it is worth a warning. Saying so per
//! refused connection is not: anything that dials in a loop — a reloading page,
//! a browser retrying a failed upgrade — can produce thousands of refusals a
//! second, and the log is not where that burst belongs.

use std::time::{Duration, Instant};

/// How long one refusal may speak for the whole burst behind it.
pub const WARN_INTERVAL: Duration = Duration::from_secs(10);

/// The rate limit on the "over cap" warning.
pub(super) struct CapWarning {
    last: Option<Instant>,
    /// Refusals since the last warning, including the current one.
    dropped: u32,
}

impl CapWarning {
    pub(super) fn new() -> Self {
        Self {
            last: None,
            dropped: 0,
        }
    }

    /// Whether this refusal is the one to log, and how many refusals that line
    /// accounts for. `None` while the burst is being suppressed.
    pub(super) fn hit(&mut self, now: Instant) -> Option<u32> {
        self.dropped = self.dropped.saturating_add(1);
        let due = self
            .last
            .is_none_or(|last| now.saturating_duration_since(last) >= WARN_INTERVAL);
        if !due {
            return None;
        }
        self.last = Some(now);
        Some(std::mem::take(&mut self.dropped))
    }
}

#[cfg(test)]
#[path = "cap_warning_tests.rs"]
mod tests;
