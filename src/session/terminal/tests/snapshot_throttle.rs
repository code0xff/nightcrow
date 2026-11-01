//! What a busy alternate-screen pane costs to keep replayable, and what a client
//! connecting between two of its snapshots is given.
//!
//! The pane, the ticks, and the comparison against a live client live next door in
//! [`desk`](super::snapshot_desk); the pace's own edges in
//! [`pace`](super::snapshot_pace).

use super::snapshot_desk::{COLS, Desk, ENTER, PANE, ROWS, assert_replay_matches_live, frame};
use crate::session::limits::MAX_TERMINAL_SCROLLBACK_BYTES;
use crate::session::terminal::hub_modes::PaneModeTracker;
use std::time::{Duration, Instant};

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

    // 32 ticks of 8 ms: the last is 256 ms after the baseline, just past the
    // window. Every tick carried output.
    for n in 0..32 {
        desk.output(&frame(n));
        desk.tick();
    }
    assert_eq!(
        desk.snapshots - baseline,
        1,
        "32 ticks of output inside one window must cost one snapshot, not one each"
    );
    assert!(
        desk.elapsed() >= Duration::from_millis(250),
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
    // 250 ms window, so only the owed bytes can have asked for the snapshot.
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
        desk.elapsed() < Duration::from_millis(250),
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

/// A chunk past the crowded cap serializes its own grid. Output after it in the
/// same tick must not make the end-of-tick pass pay for the same serialization
/// again.
#[test]
fn a_crowded_snapshot_is_not_taken_again_in_the_same_tick() {
    let dir = tempfile::TempDir::new().unwrap();
    let mut desk = Desk::new(&dir);
    desk.output(ENTER);
    desk.tick();
    let baseline = desk.snapshots;

    // Let the window pass with no output, so the only anchor the paced pass
    // could use is the old entry screen rather than the crowded chunk's own.
    for _ in 0..40 {
        desk.tick();
    }
    let crowded = vec![b'z'; MAX_TERMINAL_SCROLLBACK_BYTES + 1];
    assert!(
        !desk.output(&crowded),
        "a chunk past the cap takes its own screen"
    );
    assert!(
        desk.output(b"more"),
        "and later output in the tick leaves the pane restless"
    );
    desk.tick();
    assert_eq!(
        desk.snapshots, baseline,
        "the paced pass must not serialize the grid the crowded chunk just stored"
    );
    assert_replay_matches_live(&desk);
    desk.hub.stop();
}

/// The chunk that takes the alternate screen carries its own screen. The
/// end-of-tick pass must not serialize that grid a second time.
#[test]
fn an_entry_chunk_screen_is_not_taken_again_at_the_end_of_its_tick() {
    let dir = tempfile::TempDir::new().unwrap();
    let hub = super::spawn_hub(&dir.path().to_string_lossy(), Vec::new(), Vec::new());
    hub.register_pane(PANE, ROWS, COLS, None, None);
    let now = Instant::now();
    let mut tracker = PaneModeTracker::default();

    assert!(
        !hub.record_pane_output(PANE, ENTER.to_vec(), &mut tracker, now),
        "a chunk that takes its own entry screen is not left restless"
    );
    assert!(
        tracker.settle_restless(&[PANE], now).is_empty(),
        "and the paced pass must not serialize that grid again"
    );
    hub.stop();
}

/// A stream that never ends at a clean boundary cannot be spliced until it is
/// twice the ring cap, where the desperate path takes the screen over the seam.
/// This is the bound the replay docs describe for torn chunks.
#[test]
fn a_torn_stream_is_snapshotted_over_the_seam_at_the_desperate_cap() {
    let dir = tempfile::TempDir::new().unwrap();
    let hub = super::spawn_hub(&dir.path().to_string_lossy(), Vec::new(), Vec::new());
    hub.register_pane(PANE, ROWS, COLS, None, None);
    let now = Instant::now();
    let mut tracker = PaneModeTracker::default();

    assert!(
        !hub.record_pane_output(PANE, ENTER.to_vec(), &mut tracker, now),
        "the entry chunk takes its own screen"
    );

    // Each chunk ends in an unfinished CSI, so the stream is torn at every
    // tick and the crowded path can never splice a screen.
    const CHUNK: usize = 4096;
    let mut chunk = vec![b'x'; CHUNK];
    chunk[CHUNK - 4..].copy_from_slice(b"\x1b[12");

    let mut whole_chunks = 0usize;
    while hub.record_pane_output(PANE, chunk.clone(), &mut tracker, now) {
        whole_chunks += 1;
        assert!(whole_chunks < 1000, "the desperate snapshot never fired");
        assert!(!tracker.at_boundary(PANE), "the stream must stay torn");
    }

    let written = (whole_chunks + 1) * CHUNK;
    assert!(
        written > 2 * MAX_TERMINAL_SCROLLBACK_BYTES,
        "the screen may only be taken over the seam past twice the cap"
    );
    assert!(
        whole_chunks * CHUNK <= 2 * MAX_TERMINAL_SCROLLBACK_BYTES,
        "and not before it"
    );
    hub.stop();
}
