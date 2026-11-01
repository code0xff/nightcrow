//! The bytes of each part `PaneReplay::capture` composes, checked against the wire
//! contract rather than the screen they happen to draw: a client concatenates the
//! frames, so a missing, repeated, or reordered byte is a corrupted recording even
//! where an emulator would have repainted over it.

use crate::runtime::emulator::PaneModes;
use crate::session::terminal::hub_helpers::PaneState;
use crate::session::terminal::hub_replay::{PaneReplay, REPLAY_CHUNK_BYTES};
use std::collections::VecDeque;
use std::sync::Arc;

const ROWS: u16 = 24;
const COLS: u16 = 80;

/// A ring whose live bytes wrap around its allocation, the shape eviction leaves
/// behind after a long attachment: `as_slices` reports a run at the end of the
/// buffer and the rest at its start.
fn wrapped(bytes: &[u8]) -> VecDeque<u8> {
    const PAD: usize = 32;
    let mut ring: VecDeque<u8> = VecDeque::with_capacity(2 * PAD);
    ring.extend(std::iter::repeat_n(b'.', PAD));
    for _ in 0..PAD {
        ring.pop_front();
    }
    ring.extend(bytes.iter().copied());
    ring
}

fn joined(chunks: &[Arc<[u8]>]) -> Vec<u8> {
    chunks
        .iter()
        .flat_map(|chunk| chunk.iter().copied())
        .collect()
}

/// What `data.chunks(REPLAY_CHUNK_BYTES)` used to cut a record into: the frames a
/// client receives have to be cut there, because one may never approach the 4 MiB
/// socket ceiling and the count has to stay inside the client queue depth.
fn assert_cut(expected: &[u8], chunks: &[Arc<[u8]>]) {
    let cuts: Vec<usize> = expected
        .chunks(REPLAY_CHUNK_BYTES)
        .map(<[u8]>::len)
        .collect();
    let sizes: Vec<usize> = chunks.iter().map(|chunk| chunk.len()).collect();
    assert_eq!(
        sizes, cuts,
        "the record must be cut where the wire contract cuts it"
    );
}

/// The normal record is `ring[..covered]`, then the snapshot, then `ring[covered..]`.
/// The mark here starts inside the wrapped ring's second run, so both halves of the
/// record have to read across the wrap without repeating a byte or skipping one.
#[test]
fn a_normal_panes_parts_are_the_history_snapshot_and_tail() {
    let ring = wrapped(&(0u8..40).collect::<Vec<_>>());
    let (front, back) = ring.as_slices();
    assert!(
        !back.is_empty() && front.len() > 5,
        "the fixture must wrap its buffer, got {} + {} bytes",
        front.len(),
        back.len()
    );
    let covered = front.len() + 5;
    assert!(covered < ring.len(), "the fixture must leave a tail");
    let snapshot = b"SNAPSHOT".to_vec();
    let modes = PaneModes::default();
    let prelude = modes.prelude();
    let pane = PaneState {
        id: 7,
        title: None,
        scrollback: ring.clone(),
        normal_screen: snapshot.clone(),
        covered,
        screen: Vec::new(),
        since: VecDeque::new(),
        rows: ROWS,
        cols: COLS,
        modes,
    };

    let replay = PaneReplay::capture(&pane);
    let [under, captured_prelude, record] = replay.parts();

    assert!(
        under.is_empty(),
        "a pane on the normal screen has nothing frozen underneath it"
    );
    assert_eq!(
        joined(captured_prelude),
        prelude,
        "the modes the program set must arrive once, whole"
    );

    let mut expected = Vec::new();
    expected.extend(ring.iter().copied().take(covered));
    expected.extend_from_slice(&snapshot);
    expected.extend(ring.iter().copied().skip(covered));
    assert_eq!(
        joined(record),
        expected,
        "the record must be the covered history, the snapshot, and the tail"
    );
    assert_cut(&expected, record);
}

/// An alternate-screen program that took over from a normal screen with history has
/// three parts: the record for the buffer it left (`1049l` and the normal record,
/// which is what the client is returned to), the prelude that switches back, and
/// its own screen followed by everything recorded since that snapshot. The tail
/// arrives through a ring that has wrapped.
#[test]
fn an_alternate_screen_panes_parts_are_the_normal_record_the_prelude_and_its_own() {
    let history = wrapped(b"HISTORY\r\nSECOND LINE\r\nTHIRD LINE\r\nFOURTH LINE\r\n");
    let (front, back) = history.as_slices();
    assert!(
        !back.is_empty(),
        "the fixture must wrap the normal ring, got {} + {} bytes",
        front.len(),
        back.len()
    );
    let covered = front.len();
    let snapshot = b"NORMAL SNAPSHOT".to_vec();
    let screen = b"\x1b[2J\x1b[36mALT SCREEN".to_vec();
    let since = wrapped(b"\x1b[3;1HALT TAIL\x1b[4;1HALT BOTTOM\x1b[5;1HALT LAST");
    assert!(
        !since.as_slices().1.is_empty(),
        "the fixture must wrap the since ring"
    );
    let modes = PaneModes {
        alt_screen: true,
        ..PaneModes::default()
    };
    let prelude = modes.prelude();
    let pane = PaneState {
        id: 7,
        title: None,
        scrollback: history.clone(),
        normal_screen: snapshot.clone(),
        covered,
        screen: screen.clone(),
        since: since.clone(),
        rows: ROWS,
        cols: COLS,
        modes,
    };

    let replay = PaneReplay::capture(&pane);
    let [under, captured_prelude, record] = replay.parts();

    let mut expected_under = Vec::new();
    expected_under.extend_from_slice(b"\x1b[?1049l");
    expected_under.extend(history.iter().copied().take(covered));
    expected_under.extend_from_slice(&snapshot);
    expected_under.extend(history.iter().copied().skip(covered));
    assert_eq!(
        joined(under),
        expected_under,
        "the buffer the program left must be rebuilt from the mark it was frozen at"
    );
    assert_cut(&expected_under, under);

    let prelude_bytes = joined(captured_prelude);
    assert_eq!(
        prelude_bytes, prelude,
        "the prelude that returns the client to the alternate screen must arrive once"
    );
    assert!(
        String::from_utf8_lossy(&prelude_bytes).contains("\x1b[?1049h"),
        "and it must lead back to the buffer the record is drawn on"
    );

    let mut expected_record = screen;
    expected_record.extend(since.iter().copied());
    assert_eq!(
        joined(record),
        expected_record,
        "the alternate record must be its screen and everything since"
    );
    assert_cut(&expected_record, record);
}
