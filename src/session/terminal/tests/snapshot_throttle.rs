//! What a busy alternate-screen pane costs to keep replayable, and what a client
//! connecting between two of its snapshots is given.
//!
//! The pane, the ticks, and the comparison against a live client live next door in
//! [`desk`](super::snapshot_desk); the pace's own edges in
//! [`pace`](super::snapshot_pace).

use super::snapshot_desk::{Desk, ENTER, assert_replay_matches_live, frame};
use crate::session::limits::MAX_TERMINAL_SCROLLBACK_BYTES;
use std::time::Duration;

/// The window is wider than a repaint: a program that redraws several times per
/// tick costs one serialization per window instead of one per clean boundary, and
/// what it sent in between is still replayed on top of that screen.
#[test]
fn many_small_writes_inside_the_window_cost_one_snapshot() {
    let dir = tempfile::TempDir::new().unwrap();
    let mut desk = Desk::new(&dir);
    desk.output(ENTER);
    desk.tick();
    let baseline = desk.snapshots;

    // 64 ticks of 8 ms: the last is 504 ms after the baseline, just past the
    // window. Every tick carried output.
    for n in 0..64 {
        desk.output(&frame(n));
        desk.tick();
    }
    assert_eq!(
        desk.snapshots - baseline,
        1,
        "64 ticks of output inside one window must cost one snapshot, not one each"
    );
    assert!(
        desk.elapsed() >= Duration::from_millis(500),
        "and the fixture must have reached the end of the window"
    );
    assert_replay_matches_live(&desk);
    desk.hub.stop();
}

/// Waiting out the window is not worth making the replay hundreds of kilobytes
/// long: past the owed cap the screen is taken as soon as the stream ends clean.
#[test]
fn output_past_the_owed_cap_is_snapshotted_inside_the_window() {
    let dir = tempfile::TempDir::new().unwrap();
    let mut desk = Desk::new(&dir);
    desk.output(ENTER);
    desk.tick();
    let baseline = desk.snapshots;

    // 1 KiB per write, a tick every eight: 80 KiB in 80 ms, well inside the
    // 500 ms window, so only the owed bytes can have asked for the snapshot.
    let chunk = vec![b'x'; 1024];
    for n in 0..80 {
        desk.output(&chunk);
        if n % 8 == 7 {
            desk.tick();
        }
    }
    desk.tick();

    assert!(
        desk.snapshots > baseline,
        "over the owed cap a snapshot must not wait for the window"
    );
    assert!(
        desk.elapsed() < Duration::from_millis(500),
        "and this fixture must still be inside it, was {:?}",
        desk.elapsed()
    );
    assert_replay_matches_live(&desk);
    desk.hub.stop();
}

/// The record is still bounded, and throttling does not change that: past the ring
/// cap a fresh screen supersedes what was owed, so a replay of a megabyte of output
/// stays close to the cap rather than growing with everything the program wrote.
#[test]
fn a_record_past_the_crowded_cap_keeps_its_replay_bounded() {
    let dir = tempfile::TempDir::new().unwrap();
    let mut desk = Desk::new(&dir);
    desk.output(ENTER);
    desk.tick();

    let chunk = vec![b'y'; 4096];
    for n in 0..256 {
        desk.output(&chunk);
        if n % 16 == 15 {
            desk.tick();
        }
    }
    desk.tick();

    let replay_bytes = assert_replay_matches_live(&desk);
    assert!(
        replay_bytes < MAX_TERMINAL_SCROLLBACK_BYTES + 64 * 1024,
        "a megabyte of output must not replay as {replay_bytes} bytes"
    );
    desk.hub.stop();
}
