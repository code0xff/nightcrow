//! The boundary tracker against the seams that matter: each case is a stream a
//! PTY read really can cut at that point, and the answer decides whether a
//! snapshot may be anchored there.

use super::PaneEmulator;
use super::boundary::StreamBoundary;

fn after(chunks: &[&[u8]]) -> StreamBoundary {
    let mut boundary = StreamBoundary::default();
    for chunk in chunks {
        boundary.feed(chunk);
    }
    boundary
}

#[test]
fn plain_text_and_complete_sequences_end_at_a_boundary() {
    for stream in [
        b"hello world\r\n".as_slice(),
        b"\x1b[2J\x1b[1;1H\x1b[38;5;208mcolour\x1b[m",
        b"\x1b]0;a title\x07after",
        b"\x1b]8;;https://example.com\x1b\\link\x1b]8;;\x1b\\",
        "한글 텍스트".as_bytes(),
        b"\x1b(B\x1b=",
        b"\x1bP1$rdata\x1b\\",
    ] {
        assert!(
            after(&[stream]).at_boundary(),
            "{:?} ends complete",
            String::from_utf8_lossy(stream)
        );
    }
}

#[test]
fn a_chunk_cut_inside_a_sequence_is_not_a_boundary() {
    for cut in [
        b"\x1b".as_slice(),
        b"\x1b[",
        b"\x1b[38;5;20",
        b"\x1b]0;a tit",
        b"\x1b]0;ends in st\x1b",
        b"\x1bP1$rdat",
        &"한".as_bytes()[..1],
        &"한".as_bytes()[..2],
    ] {
        assert!(
            !after(&[cut]).at_boundary(),
            "{cut:?} is mid-sequence and must defer the anchor"
        );
    }
}

#[test]
fn the_cut_sequence_completes_in_the_next_chunk() {
    for (cut, rest) in [
        (b"\x1b[2".as_slice(), b"J".as_slice()),
        (b"\x1b]0;tit", b"le\x07"),
        (b"\x1b]0;st ends it\x1b", b"\\"),
        (&"한".as_bytes()[..2], &"한".as_bytes()[2..]),
    ] {
        let cut_only = after(&[cut]);
        assert!(!cut_only.at_boundary());
        assert!(
            after(&[cut, rest]).at_boundary(),
            "{rest:?} closes what {cut:?} opened"
        );
    }
}

#[test]
fn an_abort_closes_the_open_sequence_the_way_the_parser_does() {
    // CAN kills a CSI outright; an ESC inside an OSC both ends the string and
    // opens whatever it introduces — here a complete CSI, so the stream is
    // clean again.
    assert!(after(&[b"\x1b[38;5\x18text"]).at_boundary());
    assert!(after(&[b"\x1b]0;cut short\x1b[2J"]).at_boundary());
    // The aborting ESC can itself be left hanging.
    assert!(!after(&[b"\x1b]0;cut short\x1b[2"]).at_boundary());
}

#[test]
fn an_open_sequence_defers_no_matter_how_long_it_runs() {
    // The tracker answers honestly; how long a deferral may go on is the
    // caller's rule (the worker's desperation threshold), not a lie told here.
    let mut boundary = StreamBoundary::default();
    boundary.feed(b"\x1b]0;");
    boundary.feed(&vec![b'x'; 64 * 1024]);
    assert!(!boundary.at_boundary());
    boundary.feed(b"\x07");
    assert!(boundary.at_boundary());
}

#[test]
fn the_emulator_reports_its_streams_boundary() {
    // The wired-through surface the hub actually asks.
    let mut emulator = PaneEmulator::new(24, 80, 0);
    emulator.process(b"prompt \x1b[38;5");
    assert!(!emulator.at_boundary());
    emulator.process(b";208m ok");
    assert!(emulator.at_boundary());
}

#[test]
fn a_synchronized_update_holds_the_boundary_until_it_ends() {
    // DEC 2026: the processor buffers the update's bytes without applying them,
    // so even a stream whose sequences are all closed is not snapshot-safe —
    // the grid does not hold those bytes yet, but the record does. The two
    // halves answer separately, because desperation may override a torn
    // sequence but never a lagging grid.
    let mut emulator = PaneEmulator::new(24, 80, 0);
    emulator.process(b"\x1b[?2026h\x1b[1;1Hheld back");
    assert!(!emulator.screen_current());
    assert!(!emulator.at_boundary());
    emulator.process(b"\x1b[?2026l");
    assert!(emulator.screen_current());
    assert!(emulator.at_boundary());
}
