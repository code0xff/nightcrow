//! Hot-file fade staging and the next stage transition, independent of
//! rendering and of the application's status state.

use std::time::{Duration, SystemTime};

/// Stages of the agent-aware focus indicator fade. `Cool` means the file is
/// outside the configured hot window and renders identically to the legacy
/// (pre-feature) row.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum HotStage {
    Fresh,
    Warm,
    Cool,
}

const FRESH_DURATION: Duration = Duration::from_secs(5);

/// Bucket a single mtime against `now` and the user's hot window. The
/// "fresh" threshold sits well above typical filesystem mtime granularity
/// (1s on FAT/older ext4) so the bold→non-bold transition remains easy to
/// spot. Negative deltas (clock skew, mtime in the future) saturate to
/// `Fresh` — the conservative "just touched" choice.
pub(crate) fn classify_hot(mtime: SystemTime, now: SystemTime, hot_window: Duration) -> HotStage {
    let age = now.duration_since(mtime).unwrap_or(Duration::ZERO);
    if age >= hot_window {
        HotStage::Cool
    } else if age < FRESH_DURATION {
        HotStage::Fresh
    } else {
        HotStage::Warm
    }
}

/// Return the next time this mtime can change the rendered stage.
///
/// A deadline is needed because the status list is now rendered only when the
/// model is dirty; waiting for another filesystem snapshot would leave the
/// accent/bold fade stale on an otherwise idle repository.
pub(crate) fn next_hot_deadline(
    mtime: SystemTime,
    now: SystemTime,
    hot_window: Duration,
) -> Option<SystemTime> {
    let age = now.duration_since(mtime).unwrap_or(Duration::ZERO);
    let transition = if age < hot_window && age < FRESH_DURATION {
        FRESH_DURATION.min(hot_window)
    } else if age < hot_window {
        hot_window
    } else {
        return None;
    };
    mtime.checked_add(transition)
}

#[cfg(test)]
#[path = "hot_stage_tests.rs"]
mod tests;
