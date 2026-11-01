//! What a whole replay costs the client's queue: the frame count at the largest pane
//! this hub allows. The 1 MiB cut is chosen so that a full hub still fits, and a client
//! attached to one receives every pane before anything else; a serializer that grew, or
//! a smaller chunk, would break that promise.

use crate::session::limits::{MAX_PANE_COLS, MAX_PANE_ROWS, MAX_PTYS_PER_REPO};
use crate::session::terminal::CLIENT_QUEUE_DEPTH;
use crate::session::terminal::hub_modes::PaneModeTracker;
use crate::session::terminal::hub_replay::REPLAY_CHUNK_BYTES;

const PANE: u32 = 7;

/// The largest screen this hub will hold: every cell a different truecolor, so the
/// serializer cannot skip a style change and the fixture stays the worst case. Measured
/// rather than assumed — if a change makes the snapshot smaller, the size check below
/// says so instead of the bound quietly proving nothing.
#[test]
fn the_largest_replay_a_hub_allows_fits_the_client_queue() {
    let (rows, cols) = (MAX_PANE_ROWS, MAX_PANE_COLS);
    let mut tracker = PaneModeTracker::default();
    let mut stream = Vec::new();
    for row in 1..=rows {
        stream.extend_from_slice(format!("\x1b[{row};1H").as_bytes());
        for col in 0..cols {
            let red = (col % 256) as u8;
            let green = (row % 256) as u8;
            stream.extend_from_slice(format!("\x1b[38;2;{red};{green};7mX").as_bytes());
        }
    }
    tracker.observe(PANE, &stream, || (rows, cols));
    let screen = tracker.snapshot(PANE).expect("the tracker saw the output");
    assert!(
        screen.len() > 4 * REPLAY_CHUNK_BYTES,
        "the fixture must be a screen several frames long, got {} bytes",
        screen.len()
    );

    // Every frame one pane's replay can need: its announcement, the mode prelude, the
    // normal buffer frozen under an alternate screen (the ring's cap and its snapshot,
    // at most two chunks), and the record — this screen plus the bytes recorded since
    // it was taken, which may run to the ring's cap, one more chunk.
    let frames_per_pane = 1 + 1 + 2 + screen.len().div_ceil(REPLAY_CHUNK_BYTES) + 1;
    // The greeting, and the completion marker's reserved slot.
    let whole_hub = frames_per_pane * MAX_PTYS_PER_REPO + 2;
    assert!(
        whole_hub <= CLIENT_QUEUE_DEPTH,
        "a {rows}x{cols} replay needs {whole_hub} frames for {MAX_PTYS_PER_REPO} panes, \
         over the {CLIENT_QUEUE_DEPTH}-frame depth"
    );
}
