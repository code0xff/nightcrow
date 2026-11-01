//! What a replay rebuilds, checked without a shell: the records are composed the
//! way the worker records them and rendered through a client emulator, and the
//! result must be the screen the live stream produces.

use super::attach;
use crate::backend::PaneId;
use crate::runtime::emulator::{PaneEmulator, PaneModes};
use crate::session::terminal::TerminalHub;
use crate::session::terminal::frame::TerminalFrame;
use crate::session::terminal::hub_helpers::PaneState;
use crate::session::terminal::hub_modes::PaneModeTracker;
use std::collections::VecDeque;
use std::time::Duration;

const ROWS: u16 = 24;
const COLS: u16 = 80;
const PANE: PaneId = 7;
const QUIET: Duration = Duration::from_millis(300);

/// One cell as a client would draw it, through the same read-only surface the
/// renderers use.
type Drawn = (String, String, bool);

fn drawn(emulator: &PaneEmulator) -> Vec<Drawn> {
    let view = emulator.view();
    let (rows, cols) = view.size();
    let mut out = Vec::with_capacity(usize::from(rows) * usize::from(cols));
    for row in 0..rows {
        for col in 0..cols {
            let Some(cell) = view.cell(row, col) else {
                continue;
            };
            let mut contents = String::new();
            cell.append_contents(&mut contents);
            let style = format!(
                "{:?}/{:?}/{}{}{}{}{}",
                cell.fg(),
                cell.bg(),
                cell.bold() as u8,
                cell.italic() as u8,
                cell.underline() as u8,
                cell.inverse() as u8,
                cell.dim() as u8,
            );
            out.push((contents, style, cell.is_wide_spacer()));
        }
    }
    out
}

fn rendered(stream: &[u8]) -> PaneEmulator {
    let mut emulator = PaneEmulator::new(ROWS, COLS, 0);
    emulator.process(stream);
    emulator
}

fn install(hub: &std::sync::Arc<TerminalHub>, pane: PaneState) {
    hub.state
        .lock()
        .expect("terminal state poisoned")
        .panes
        .push(pane);
}

/// A ring whose live bytes wrap around the end of its buffer, with `evicted`
/// bytes already drained from the front — what eviction leaves behind after a
/// long attachment.
fn wrapped_ring(stream: &[u8], evicted: usize) -> VecDeque<u8> {
    let mut ring: VecDeque<u8> = VecDeque::with_capacity(64);
    const HEAD: usize = 32;
    ring.extend(std::iter::repeat_n(b' ', HEAD));
    for _ in 0..HEAD {
        ring.pop_front();
    }
    ring.extend(stream.iter().copied());
    ring.drain(..evicted);
    ring
}

fn replay_bytes(hub: &std::sync::Arc<TerminalHub>) -> Vec<u8> {
    let session = attach(hub);
    let mut out = Vec::new();
    while let Some(frame) = session.next_frame(QUIET) {
        if let TerminalFrame::Output { pane, data } = frame
            && pane == PANE
        {
            out.extend(data.iter().copied());
        }
    }
    out
}

fn assert_same_screen(replay: &[u8], live: &[u8]) {
    let from_replay = rendered(replay);
    let from_live = rendered(live);
    assert_eq!(
        drawn(&from_replay),
        drawn(&from_live),
        "the replay must land on the screen the live stream produced"
    );
    assert_eq!(
        from_replay.view().cursor_position(),
        from_live.view().cursor_position(),
        "and the cursor must be in the same cell"
    );
}

/// The ring is a byte window, so its front may be the tail of an escape whose
/// introducer was already evicted. The snapshot repaints over that fragment; the
/// screen must still arrive whole.
#[test]
fn a_normal_replay_whose_history_starts_mid_escape_lands_the_live_screen() {
    let shown = b"\x1b[31mRED HISTORY\x1b[0m\r\nSECOND LINE\r\n";
    let tail = b"\x1b[3;1H> ";

    let mut tracker = PaneModeTracker::default();
    let observed = tracker.observe(PANE, shown, || (ROWS, COLS));
    let snapshot = tracker.snapshot(PANE).expect("the tracker saw the output");

    // One byte of the sequence's introducer is gone: the ring begins `[31mRED`.
    let mut ring = wrapped_ring(shown, 1);
    assert!(
        !ring.as_slices().1.is_empty(),
        "the fixture must wrap around its buffer"
    );
    let covered = ring.len();
    ring.extend(tail.iter().copied());

    let dir = tempfile::TempDir::new().unwrap();
    let hub = super::spawn_hub(&dir.path().to_string_lossy(), Vec::new(), Vec::new());
    install(
        &hub,
        PaneState {
            id: PANE,
            title: None,
            scrollback: ring,
            normal_screen: snapshot,
            covered,
            screen: Vec::new(),
            since: VecDeque::new(),
            rows: ROWS,
            cols: COLS,
            modes: observed.modes,
        },
    );

    let replay = replay_bytes(&hub);
    hub.stop();

    let text = String::from_utf8_lossy(&replay).to_string();
    assert!(
        text.contains("[31mRED HISTORY"),
        "the ring must be replayed from inside the escaped sequence, got: {text:?}"
    );
    let mut live = shown.to_vec();
    live.extend_from_slice(tail);
    assert_same_screen(&replay, &live);
}

#[test]
fn an_alternate_screen_replay_lands_the_live_screen() {
    let shown = b"\x1b[?1049h\x1b[2J\x1b[1;1mALT TOP\x1b[5;3mALT BOTTOM";
    let tail = b"\x1b[8;1mALT TAIL";

    let mut tracker = PaneModeTracker::default();
    let observed = tracker.observe(PANE, shown, || (ROWS, COLS));
    assert!(
        observed.modes.alt_screen,
        "the fixture must enter the alt screen"
    );
    let screen = tracker.snapshot(PANE).expect("the tracker saw the output");

    let dir = tempfile::TempDir::new().unwrap();
    let hub = super::spawn_hub(&dir.path().to_string_lossy(), Vec::new(), Vec::new());
    install(
        &hub,
        PaneState {
            id: PANE,
            title: None,
            scrollback: VecDeque::new(),
            normal_screen: Vec::new(),
            covered: 0,
            screen,
            since: tail.iter().copied().collect(),
            rows: ROWS,
            cols: COLS,
            modes: PaneModes {
                alt_screen: true,
                ..observed.modes
            },
        },
    );

    let replay = replay_bytes(&hub);
    hub.stop();

    let mut live = shown.to_vec();
    live.extend_from_slice(tail);
    assert_same_screen(&replay, &live);
}
