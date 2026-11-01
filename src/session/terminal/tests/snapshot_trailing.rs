//! What a pane's tail costs after it falls idle: the state a snapshot does not
//! carry stays exposed only until the trailing snapshot the pace takes one
//! window after the pane's last write.
//!
//! A program that sets a scroll region and then scrolls inside it leaves bytes
//! that reproduce its screen only while that region is still there. The screen
//! snapshot is an absolute repaint and carries no DECSTBM, so replaying those
//! bytes without the trailing snapshot lands them on a full-screen region. The
//! trailing pass folds them into the snapshot instead.

use super::snapshot_desk::{Desk, assert_replay_matches_live};
use std::time::Duration;

/// Enough 8 ms ticks to cross the 250 ms window and then leave the pane idle.
const IDLE_TICKS: usize = 40;

#[test]
fn a_scroll_region_set_before_a_snapshot_is_folded_into_a_trailing_one() {
    let dir = tempfile::TempDir::new().unwrap();
    let mut desk = Desk::new(&dir);

    // The chunk that enters the alternate screen also sets the scroll region
    // the program keeps writing through, so the screen taken at its end is
    // painted into a grid the snapshot cannot describe.
    desk.output(b"\x1b[?1049h\x1b[2J\x1b[1;20r\x1b[1;1Htop\x1b[20;1Hbottom");
    desk.tick();
    let baseline = desk.snapshots;

    // Inside the window: newlines at the bottom margin scroll only the region.
    // Those bytes are owed on top of the snapshot until the pass takes one.
    desk.output(b"\x1b[20;1H\r\n\r\n\r\nscrolled");
    desk.tick();
    assert_eq!(
        desk.snapshots, baseline,
        "inside the window the pane keeps its tail rather than paying for a screen"
    );

    // Go idle across the window. Nothing else will revisit the pane, so the
    // pass must: the tail is what carries state the snapshot does not.
    for _ in 0..IDLE_TICKS {
        desk.tick();
    }
    assert_eq!(
        desk.snapshots - baseline,
        1,
        "an idle pane gets exactly one trailing snapshot"
    );
    assert_replay_matches_live(&desk);

    // It owes nothing after that, so a later idle window must not serialize the
    // same unchanged grid again.
    for _ in 0..IDLE_TICKS {
        desk.tick();
    }
    assert_eq!(
        desk.snapshots - baseline,
        1,
        "and none afterwards, because there is nothing left to fold in"
    );
    assert!(
        desk.elapsed() >= Duration::from_millis(250),
        "the fixture must have crossed the window"
    );
    desk.hub.stop();
}
