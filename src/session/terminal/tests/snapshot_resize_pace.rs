//! A resize replaces the grid the last screen painted. The pace must notice.
//!
//! Inside a window, the old snapshot is still an exact repaint of the old grid
//! and cannot anchor the new one. Clearing the pane's pace at the resize lets
//! the next clean boundary take a screen instead of waiting out a period that
//! was measured against the record it replaced.

use super::snapshot_desk::{Desk, ENTER, assert_replay_matches_live};
use std::time::Duration;

#[test]
fn a_torn_resize_snapshots_at_the_next_clean_boundary_inside_the_window() {
    let dir = tempfile::TempDir::new().unwrap();
    let mut desk = Desk::new(&dir);
    desk.output(ENTER);
    desk.tick();
    let baseline = desk.snapshots;

    // A cursor position cut in half: the resize cannot splice a screen here.
    desk.output(b"\x1b[2;1HTORN\x1b[");
    desk.tick();
    assert_eq!(
        desk.snapshots, baseline,
        "a torn stream is no place for the resize's screen"
    );

    // The resize lands inside the window and still has no clean boundary to
    // take, so it defers. Its grid, not the old snapshot, is what a client must
    // now be shown.
    desk.resize(20, 60);
    assert_eq!(
        desk.snapshots, baseline,
        "the torn stream defers the resize's own screen"
    );

    // The next chunk closes the sequence. The cleared pace makes this clean
    // boundary snapshot even though the window has not passed.
    desk.output(b"1;9HCLOSED");
    desk.tick();
    assert_eq!(
        desk.snapshots - baseline,
        1,
        "the next clean boundary takes the resized grid"
    );
    assert!(
        desk.elapsed() < Duration::from_millis(250),
        "and it must do so inside the window, not after it"
    );
    assert_replay_matches_live(&desk);
    desk.hub.stop();
}
