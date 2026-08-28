//! Recording where a pane's screen-clearing input came from. This exists
//! because of a specific unexplained event — a pane's conversation cleared
//! fourteen times in five seconds, the clears arriving as `0x0c` at a
//! machine-like cadence nobody could attribute. nightcrow does not synthesize
//! that byte, which leaves a client's own input: so this notes the arrival and
//! its shape, and the client says what produced it
//! (`ClientMessage::ClearKeyReport`, logged in `session.rs`).
//!
//! **No input content is logged, ever** — only the byte's count, how much else
//! rode with it, and the timing.

use crate::backend::PaneId;
use std::collections::HashMap;
use std::time::{Duration, Instant};

/// Form feed: what `Ctrl+L` sends, and what a terminal program takes as "draw
/// the screen again".
pub(super) const CLEAR_SCREEN: u8 = 0x0c;

/// Quiet gap that ends a burst. Two seconds is Claude Code's own window for
/// treating a second `Ctrl+L` as `/clear`.
pub(super) const BURST_GAP: Duration = Duration::from_secs(2);

/// Lines one burst may write before the rest are counted silently. A held key
/// repeats tens of times a second; the shape is clear long before that.
pub(super) const MAX_LINES_PER_BURST: u32 = 40;

struct Burst {
    last: Instant,
    seen: u32,
    logged: u32,
}

/// Per-pane burst state for the arrival log.
#[derive(Default)]
pub(super) struct ClearWatch {
    bursts: HashMap<PaneId, Burst>,
}

impl ClearWatch {
    /// Note an input frame on its way to `pane` and log what it says.
    pub(super) fn note_input(&mut self, pane: PaneId, client: u64, data: &[u8], now: Instant) {
        let Some(note) = self.record(pane, data, now) else {
            return;
        };
        if let Some(total) = note.previous_burst_total {
            tracing::info!(pane, total, "viewer: end of a run of screen-clearing input");
        }
        if note.suppressed {
            return;
        }
        tracing::info!(
            pane,
            client,
            clears = note.clears,
            // Whether anything rode along with it. A `Ctrl+L` from a keyboard
            // arrives alone; a paste or a script writing a block does not.
            other_bytes = note.other_bytes,
            gap_ms = note.gap_ms,
            in_burst = note.in_burst,
            "viewer: a client sent the screen-clearing byte"
        );
    }

    /// The counting behind [`ClearWatch::note_input`], kept separate from the
    /// logging so the burst arithmetic can be tested rather than read.
    ///
    /// `None` when the frame carries no clear byte.
    pub(super) fn record(&mut self, pane: PaneId, data: &[u8], now: Instant) -> Option<ClearNote> {
        let clears = data.iter().filter(|&&b| b == CLEAR_SCREEN).count() as u32;
        if clears == 0 {
            return None;
        }
        let burst = self.bursts.entry(pane).or_insert(Burst {
            last: now,
            seen: 0,
            logged: 0,
        });
        let gap = now.duration_since(burst.last);
        let mut previous_burst_total = None;
        if gap > BURST_GAP && burst.seen > 0 {
            // Worth a line only when the run outgrew what was logged; otherwise
            // every line of it is already above.
            if burst.seen > burst.logged {
                previous_burst_total = Some(burst.seen);
            }
            burst.seen = 0;
            burst.logged = 0;
        }
        burst.last = now;
        burst.seen += clears;
        let suppressed = burst.logged >= MAX_LINES_PER_BURST;
        if !suppressed {
            burst.logged += clears;
        }
        Some(ClearNote {
            clears,
            other_bytes: data.len() as u32 - clears,
            gap_ms: gap.as_millis() as u64,
            in_burst: burst.seen,
            suppressed,
            previous_burst_total,
        })
    }

    /// Drop a gone pane's burst state.
    pub(super) fn forget(&mut self, pane: PaneId) {
        self.bursts.remove(&pane);
    }
}

/// What one clear-bearing frame amounts to.
#[derive(Debug, PartialEq, Eq)]
pub(super) struct ClearNote {
    pub(super) clears: u32,
    pub(super) other_bytes: u32,
    pub(super) gap_ms: u64,
    /// Running total for the burst this frame belongs to.
    pub(super) in_burst: u32,
    /// This one is past the per-burst line budget and only counted.
    pub(super) suppressed: bool,
    /// The run that just ended, when this frame started a new one after a
    /// suppressed tail.
    pub(super) previous_burst_total: Option<u32>,
}
