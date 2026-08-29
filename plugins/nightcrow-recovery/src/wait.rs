//! Waiting for a usage limit to reset, without trusting either clock alone.
//!
//! A reset time is an absolute unix second, but the wall clock can be changed
//! underneath a wait that lasts hours — an NTP correction, a laptop returning
//! from suspend. A wait driven purely by wall time would then fire early
//! (resuming into a limit that has not cleared, burning an attempt) or never
//! fire at all (stranding the pane).
//!
//! So a wait keeps both: the absolute deadline, and a monotonic countdown of
//! the same length. When the two disagree by more than [`JUMP_TOLERANCE_SECS`]
//! the wall clock moved, and the deadline is shifted by that amount so the
//! real time spent waiting can be neither shortened nor lengthened.

use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

/// Disagreement between the wall and monotonic clocks that counts as the wall
/// clock having been changed.
///
/// Poll-to-poll deltas are compared at whole-second resolution, so a couple of
/// seconds of rounding is normal; five is comfortably above that and far below
/// any correction a human or NTP would make worth reacting to.
pub const JUMP_TOLERANCE_SECS: i64 = 5;

/// Shortest wait, applied even when the reported reset time is already past.
///
/// A reset time can be stale by the time it reaches us. Resuming in the same
/// breath as noticing the limit would spend an attempt on a limit that has not
/// actually cleared, so every wait has a floor.
pub const MIN_WAIT_SECS: i64 = 15;

/// Longest wait. A deadline beyond eight days is clamped rather than parking a
/// pane indefinitely. Must stay under the host's
/// `PENDING_RELAUNCH_TTL` (nine days): a wait outlasting that would end with
/// nothing left to resume.
pub const MAX_WAIT_SECS: i64 = 8 * 24 * 60 * 60;

/// Added to a reported reset time before resuming.
///
/// Providers report the second a window rolls over, and a request landing on
/// that exact second is still liable to be refused. Half a minute costs nothing
/// against a multi-hour window and avoids spending an attempt on the boundary.
pub const RESET_GRACE_SECS: i64 = 30;

/// First backoff step, used when no reset time is known.
///
/// Long enough that a provider's own transient failure has passed, short enough
/// that a human watching the pane sees progress.
pub const BACKOFF_BASE_SECS: i64 = 30;

/// Ceiling on the doubling. Past half an hour a blind retry is no more likely to
/// succeed, and the machine's attempt bound will end the sequence anyway.
pub const BACKOFF_MAX_SECS: i64 = 30 * 60;

/// Current unix second, or 0 if the system clock predates the epoch — a value
/// every reset-time check treats as implausible, which is the safe reading.
pub fn now_epoch() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

/// A pending wait. Created once, polled on the plugin's timer.
#[derive(Debug, Clone)]
pub struct ResetWait {
    /// The deadline as this process now believes it, in unix seconds. Shifted
    /// when a wall-clock jump is detected.
    deadline_epoch: i64,
    /// How long the wait should really last, fixed when the wait was made.
    planned: Duration,
    started: Instant,
    last_epoch: i64,
    last_mono: Instant,
}

impl ResetWait {
    /// Wait until `deadline_epoch` (plus [`RESET_GRACE_SECS`]), clamped to
    /// [`MIN_WAIT_SECS`]..=[`MAX_WAIT_SECS`].
    pub fn until(deadline_epoch: i64, now_epoch: i64, now: Instant) -> Self {
        let target = deadline_epoch.saturating_add(RESET_GRACE_SECS);
        let planned = (target - now_epoch).clamp(MIN_WAIT_SECS, MAX_WAIT_SECS);
        Self {
            deadline_epoch: now_epoch.saturating_add(planned),
            planned: Duration::from_secs(planned as u64),
            started: now,
            last_epoch: now_epoch,
            last_mono: now,
        }
    }

    /// Wait out an exponential backoff step, for when no reset time is known.
    /// `attempt` counts from 1.
    pub fn backoff(attempt: u32, now_epoch: i64, now: Instant) -> Self {
        // `attempt - 1` doublings, saturating so a large attempt count cannot
        // overflow the shift; the cap makes anything past a few steps identical.
        let shift = attempt.saturating_sub(1).min(u32::BITS - 2);
        let secs = BACKOFF_BASE_SECS
            .saturating_mul(1i64 << shift)
            .clamp(MIN_WAIT_SECS, BACKOFF_MAX_SECS);
        // Built directly rather than through `until`: a backoff is a duration we
        // chose, so it needs no reset grace and no provider-reported time.
        let deadline = now_epoch.saturating_add(secs);
        Self {
            deadline_epoch: deadline,
            planned: Duration::from_secs(secs as u64),
            started: now,
            last_epoch: now_epoch,
            last_mono: now,
        }
    }

    /// When this wait expects to end, in unix seconds on the clock as currently
    /// understood. This is what the host displays.
    pub fn deadline_epoch(&self) -> i64 {
        self.deadline_epoch
    }

    /// Whether the wait is over. Also absorbs any wall-clock jump since the last
    /// call, so this must be called on a timer rather than only when the answer
    /// is wanted.
    pub fn poll(&mut self, now_epoch: i64, now: Instant) -> bool {
        let mono_delta = now.duration_since(self.last_mono).as_secs() as i64;
        let jump = (now_epoch - self.last_epoch) - mono_delta;
        if jump.abs() >= JUMP_TOLERANCE_SECS {
            // Keep the deadline pinned to real elapsed time rather than to the
            // number the wall clock now shows.
            self.deadline_epoch = self.deadline_epoch.saturating_add(jump);
        }
        self.last_epoch = now_epoch;
        self.last_mono = now;
        now_epoch >= self.deadline_epoch && now.duration_since(self.started) >= self.planned
    }
}

#[cfg(test)]
#[path = "wait_tests.rs"]
mod tests;
