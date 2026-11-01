//! How much queued output may wait for a flush, and when it stops waiting.
//!
//! Without a bound, a program that never pauses would leave every frame it
//! produced sitting behind a burst that never ends. Without the age bound, a
//! steady trickle slower than the poll would sit there too.

use std::time::{Duration, Instant};

/// Frames one batch may hold before it is flushed.
pub(super) const FLUSH_FRAMES: usize = 64;
/// Bytes one batch may hold before it is flushed, whichever bound comes first.
pub(super) const FLUSH_BYTES: usize = 32 * 1024;
/// Age at which a batch is flushed even if neither bound was reached.
pub(super) const FLUSH_AFTER: Duration = Duration::from_millis(4);

/// Frames queued since the last flush, and whether one is due.
#[derive(Default)]
pub(super) struct Batch {
    frames: usize,
    bytes: usize,
    started: Option<Instant>,
    due: bool,
}

impl Batch {
    pub(super) fn queued(&self) -> bool {
        self.frames > 0
    }

    pub(super) fn due(&self) -> bool {
        self.due
    }

    pub(super) fn add(&mut self, bytes: usize) {
        self.frames += 1;
        self.bytes += bytes;
        let started = *self.started.get_or_insert_with(Instant::now);
        self.due |= self.frames >= FLUSH_FRAMES
            || self.bytes >= FLUSH_BYTES
            || started.elapsed() >= FLUSH_AFTER;
    }

    pub(super) fn clear(&mut self) {
        *self = Self::default();
    }
}

#[cfg(test)]
#[path = "terminal_batch_tests.rs"]
mod tests;
