//! How much history a client gets back when it attaches.
//!
//! Two caps meet here and neither knows about the other: the hub keeps a
//! byte-bounded ring per pane ([`limits::MAX_TERMINAL_SCROLLBACK_BYTES`]) and a
//! client keeps a line-bounded history
//! ([`SCROLLBACK_LINES`](crate::runtime::terminal::SCROLLBACK_LINES)). Whether a
//! replay can fill the line cap therefore depends on bytes per line, which
//! nothing enforces — so it is measured.
//!
//! Measured against the ring itself rather than a shell: what is in question is
//! the ratio between the two caps, and the delivery path is covered by the
//! reconnect tests. Driving a real pane hard enough to saturate the ring would
//! also be a test that floods the very client watching it.

use crate::runtime::emulator::PaneEmulator;
use crate::runtime::terminal::SCROLLBACK_LINES;
use crate::web::viewer::limits;
use crate::web::viewer::terminal::hub_helpers::push_scrollback;
use std::collections::VecDeque;

/// Rows and columns of the pane the replay is measured into. The width decides
/// how many rows a line takes, so a narrow pane would flatter the result.
const ROWS: u16 = 24;
const COLS: u16 = 80;

/// Push `lines` copies of `line` through the hub's own eviction, then replay the
/// ring into a fresh client's emulator and report the history it ends up with.
fn depth_after_replay(line: &str, lines: usize) -> usize {
    let mut ring: VecDeque<u8> = VecDeque::new();
    for _ in 0..lines {
        push_scrollback(&mut ring, line.as_bytes());
    }
    assert_eq!(
        ring.len(),
        limits::MAX_TERMINAL_SCROLLBACK_BYTES,
        "the ring must be saturated for this to measure anything"
    );
    let replay: Vec<u8> = ring.iter().copied().collect();
    let mut emulator = PaneEmulator::new(ROWS, COLS, SCROLLBACK_LINES);
    emulator.process(&replay);
    emulator.set_scroll_offset(usize::MAX)
}

#[test]
fn a_replay_of_plain_output_fills_a_client_scrollback() {
    // The ordinary case: shell output, one screen wide. The byte window is far
    // wider than the client's line cap here, so the client's own history is what
    // runs out first — a client that attaches gets all the scrollback it can
    // hold.
    let line = format!("{}\r\n", "n".repeat(COLS as usize - 1));
    let lines = limits::MAX_TERMINAL_SCROLLBACK_BYTES / line.len() + 100;

    assert_eq!(depth_after_replay(&line, lines), SCROLLBACK_LINES);
}

#[test]
fn output_with_more_escapes_than_text_is_replayed_shallower_than_it_was_kept() {
    // The other side of the ratio, which is the answer to "can the byte cap
    // starve the line cap": yes, past this many bytes per line. Heavily
    // syntax-highlighted output reaches it — a colour change per token can cost
    // several times the characters it paints.
    //
    // Left as it is rather than raising the byte cap. The output that gets there
    // is mostly repaint sequences rather than text, the shortfall is a few
    // hundred lines out of a thousand, and the cap is paid per pane per
    // repository — a wider window would be memory spent on the case that needs it
    // least.
    let per_line = limits::MAX_TERMINAL_SCROLLBACK_BYTES / SCROLLBACK_LINES + 1;
    let escapes = "\x1b[32mx\x1b[0m".repeat(per_line / 10);
    let line = format!("{escapes}\r\n");
    let lines = limits::MAX_TERMINAL_SCROLLBACK_BYTES / line.len() + 100;

    let depth = depth_after_replay(&line, lines);
    assert!(
        depth < SCROLLBACK_LINES,
        "{depth} lines came back from {}-byte lines, which cannot be",
        line.len()
    );
    // Still most of it, which is why nothing is changed here.
    assert!(
        depth > SCROLLBACK_LINES / 2,
        "only {depth} lines survived — the two caps have drifted apart"
    );
}
