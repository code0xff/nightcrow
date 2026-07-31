//! What a pane's modes are, and that the prelude reproduces them.
//!
//! The round-trip test is the contract: whatever a program set, feeding a fresh
//! terminal the prelude must leave it in the same state — that is the whole
//! promise made to a client attaching to a pane whose startup bytes are gone.

use super::PaneModes;
use crate::runtime::emulator::PaneEmulator;

const ROWS: u16 = 24;
const COLS: u16 = 80;

fn modes_after(sequences: &[u8]) -> PaneModes {
    let mut emulator = PaneEmulator::new(ROWS, COLS, 0);
    emulator.process(sequences);
    emulator.modes()
}

/// Feed `sequences` to one terminal, hand its prelude to a fresh one, and
/// return both states so the caller can require they match.
fn round_trip(sequences: &[u8]) -> (PaneModes, PaneModes) {
    let established = modes_after(sequences);
    let mut fresh = PaneEmulator::new(ROWS, COLS, 0);
    fresh.process(&established.prelude());
    (established, fresh.modes())
}

#[test]
fn a_freshly_opened_pane_matches_the_default() {
    // `PaneState` starts every pane at the default, so it has to be what the
    // emulator actually opens with — including modes that start *on*. An
    // emulator upgrade that changes its initial mode set fails here.
    let fresh = PaneEmulator::new(ROWS, COLS, 0);
    assert_eq!(fresh.modes(), PaneModes::default());
}

#[test]
fn the_prelude_states_every_mode_either_way() {
    // Absolute, not relative to a fresh terminal: the receiver is xterm.js and
    // its defaults are not this emulator's to assume.
    let prelude = String::from_utf8(PaneModes::default().prelude()).unwrap();

    assert!(prelude.contains("\x1b[?1049l"), "got: {prelude:?}");
    assert!(prelude.contains("\x1b[?25h"), "got: {prelude:?}");
    assert!(prelude.contains("\x1b[?2004l"), "got: {prelude:?}");
    let (_, restored) = round_trip(b"");
    assert_eq!(restored, PaneModes::default());
}

#[test]
fn plain_output_leaves_the_modes_alone() {
    assert_eq!(modes_after(b"hello\r\nworld\r\n"), PaneModes::default());
}

#[test]
fn a_fullscreen_programs_modes_survive_the_prelude() {
    // What Claude Code in fullscreen rendering sets on startup: alternate
    // screen, SGR mouse reporting with drags, bracketed paste, app cursor.
    let (established, restored) =
        round_trip(b"\x1b[?1049h\x1b[?1002h\x1b[?1006h\x1b[?2004h\x1b[?1h");

    assert!(established.alt_screen);
    assert!(established.mouse_drag);
    assert!(established.sgr_mouse);
    assert!(established.bracketed_paste);
    assert!(established.app_cursor);
    assert_eq!(
        restored, established,
        "the prelude must reproduce every mode the program set"
    );
}

#[test]
fn modes_a_program_turned_off_are_restored_too() {
    // 25 and 7 are on in a fresh terminal, so restoring them means emitting the
    // reset — the case an "only emit what is set" prelude would miss.
    let (established, restored) = round_trip(b"\x1b[?25l\x1b[?7l");

    assert!(!established.show_cursor);
    assert!(!established.line_wrap);
    assert_eq!(restored, established);
}

#[test]
fn leaving_the_alternate_screen_clears_the_flag() {
    let (established, restored) = round_trip(b"\x1b[?1049h\x1b[?1049l");

    assert!(
        !established.alt_screen,
        "a program that left the alternate screen is on the normal one"
    );
    assert_eq!(restored, established);
    let prelude = String::from_utf8(established.prelude()).unwrap();
    assert!(
        prelude.contains("\x1b[?1049l"),
        "the prelude must put the client back on the normal buffer, got: {prelude:?}"
    );
}

#[test]
fn the_alternate_screen_switch_leads_the_prelude() {
    let modes = modes_after(b"\x1b[?1006h\x1b[?1049h");
    let prelude = String::from_utf8(modes.prelude()).unwrap();

    // Every other mode has to land in the buffer the program is drawing on, so
    // the buffer switch cannot come second.
    assert!(
        prelude.starts_with("\x1b[?1049h"),
        "prelude must open with the buffer switch, got: {prelude:?}"
    );
    assert!(prelude.contains("\x1b[?1006h"));
}

#[test]
fn every_tracked_mode_appears_in_the_prelude() {
    let all = b"\x1b[?1049h\x1b[?1h\x1b[?7l\x1b[?25l\x1b[?1000h\x1b[?1002h\
\x1b[?1003h\x1b[?1004h\x1b[?1005h\x1b[?1006h\x1b[?1007h\x1b[?2004h";
    let (established, restored) = round_trip(all);

    assert_eq!(restored, established);
    let prelude = String::from_utf8(established.prelude()).unwrap();
    for number in [
        1049, 1, 7, 25, 1000, 1002, 1003, 1004, 1005, 1006, 1007, 2004,
    ] {
        assert!(
            prelude.contains(&format!("\x1b[?{number}")),
            "mode {number} missing from {prelude:?}"
        );
    }
}
