use super::*;
use std::time::{Duration, Instant};

/// vte's own window for an open synchronized update. Mirrored rather than
/// imported — the crate keeps it private, and the contract this exercises is
/// that an abandoned update ends on *some* clock the owner can reach.
const SYNC_WINDOW: Duration = Duration::from_millis(150);

fn line(view: &ScreenView<'_>, row: u16, cols: u16) -> String {
    let mut out = String::new();
    for col in 0..cols {
        view.cell(row, col).unwrap().append_contents(&mut out);
    }
    out.trim_end().to_string()
}

#[test]
fn an_update_the_program_never_closed_ends_on_the_clock() {
    // The freeze this exists for: a TUI killed between BSU and ESU, and a pane
    // that afterwards produces only a prompt — far too little to reach the
    // processor's buffer cap, so nothing but the clock ever lets the grid move
    // again.
    let mut emu = PaneEmulator::new(3, 20, 0);
    emu.process(b"\x1b[?2026h\x1b[1;1Hheld back");
    assert!(!emu.screen_current());
    assert_eq!(line(&emu.view(), 0, 20), "");

    let expired = Instant::now() + SYNC_WINDOW;
    assert!(emu.sync_expired(expired));
    emu.settle_sync();

    assert!(emu.screen_current());
    assert!(emu.at_boundary());
    assert_eq!(line(&emu.view(), 0, 20), "held back");
}

#[test]
fn an_update_still_inside_its_window_is_left_open() {
    let mut emu = PaneEmulator::new(3, 20, 0);
    emu.process(b"\x1b[?2026h\x1b[1;1Hheld back");

    assert!(!emu.sync_expired(Instant::now()));
}

#[test]
fn a_pane_with_no_update_open_has_nothing_to_settle() {
    let mut emu = PaneEmulator::new(3, 20, 0);
    emu.process(b"plain output");

    assert!(!emu.sync_expired(Instant::now() + SYNC_WINDOW));
    assert!(emu.screen_current());
}

#[test]
fn a_settled_update_reports_the_title_it_carried() {
    // Side effects buffered with the update are not lost by settling it: they
    // reach the caller the same way a processed chunk's would.
    let mut emu = PaneEmulator::new(3, 20, 0);
    emu.process(b"\x1b[?2026h\x1b]2;held title\x07");

    let events = emu.settle_sync();

    assert_eq!(events.title.as_deref(), Some("held title"));
}
