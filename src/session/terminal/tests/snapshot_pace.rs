//! The pace itself: when a busy alternate-screen pane's screen is due, and what
//! taking it costs.
//!
//! The behavioural half — that a replay taken between two snapshots still rebuilds
//! the live screen — is next door, in
//! [`throttle`](super::snapshot_throttle). These tests hold the two thresholds to
//! their exact edges, pin the verdict the worker's loop acts on, and measure what
//! the pace saves.

use super::snapshot_desk::{
    COLS, Desk, ENTER, PANE, ROWS, TICK, assert_replay_matches_live, frame,
};
use crate::session::limits::MAX_TERMINAL_SCROLLBACK_BYTES;
use crate::session::terminal::hub_modes::PaneModeTracker;
use std::time::{Duration, Instant};

/// The window. Restated rather than read because "one tick short of it" is the
/// point of the test.
const WINDOW: Duration = Duration::from_millis(250);

/// The owed output that makes waiting out the window pointless.
const OWED_CAP: usize = 64 * 1024;

/// Past the 150 ms window a synchronized update is armed with.
const SYNC_WINDOW: Duration = Duration::from_millis(200);

/// A tracker holding one alternate-screen pane whose screen was just taken at
/// `now`.
fn snapshotted_pane(now: Instant) -> PaneModeTracker {
    let mut tracker = PaneModeTracker::default();
    tracker.observe(PANE, ENTER, || (ROWS, COLS));
    tracker.note_owed(PANE, 0);
    assert_eq!(tracker.settle_restless(&[PANE], now).len(), 1);
    tracker
}

/// The two thresholds meet at their edges: a snapshot is due exactly one window
/// after the last one, and due early only once the record owes *more* than the cap.
#[test]
fn the_pace_is_due_at_the_window_and_only_past_the_owed_cap() {
    let now = Instant::now();
    let mut tracker = snapshotted_pane(now);

    tracker.note_owed(PANE, OWED_CAP);
    assert!(
        tracker
            .settle_restless(&[PANE], now + WINDOW - TICK)
            .is_empty(),
        "the cap is exceeded, not reached, and the window has not passed"
    );
    tracker.note_owed(PANE, OWED_CAP + 1);
    assert_eq!(
        tracker.settle_restless(&[PANE], now + WINDOW - TICK).len(),
        1,
        "one byte past the cap is due inside the window"
    );

    let taken = now + WINDOW - TICK;
    tracker.note_owed(PANE, 0);
    assert!(
        tracker.settle_restless(&[PANE], taken + TICK).is_empty(),
        "a screen just taken is not due again on the next tick"
    );
    assert_eq!(
        tracker.settle_restless(&[PANE], taken + WINDOW).len(),
        1,
        "and it is due exactly one window later"
    );
}

/// The worker no longer ticks, so the pace has to say when it is next due: the
/// end of the running window, and nothing once the pane owes nothing or its
/// stream is mid-sequence.
#[test]
fn an_owed_pane_names_the_end_of_its_window_and_nothing_after_it() {
    let now = Instant::now();
    let mut tracker = snapshotted_pane(now);
    assert_eq!(
        tracker.next_deadline(),
        None,
        "a pane owing nothing has no trailing snapshot coming"
    );

    tracker.note_owed(PANE, 1);
    assert_eq!(
        tracker.next_deadline(),
        Some(now + WINDOW),
        "the trailing snapshot is due exactly one window after the last screen"
    );

    assert_eq!(tracker.settle_restless(&[PANE], now + WINDOW).len(), 1);
    assert_eq!(
        tracker.next_deadline(),
        None,
        "the pass that took it must leave no deadline behind"
    );
}

/// A pane nothing has been snapshotted for has no window to wait out.
#[test]
fn a_pane_the_tracker_has_never_snapshotted_is_due_at_once() {
    let now = Instant::now();
    let mut tracker = PaneModeTracker::default();
    tracker.observe(PANE, ENTER, || (ROWS, COLS));
    tracker.note_owed(PANE, 0);

    assert_eq!(tracker.settle_restless(&[PANE], now).len(), 1);
}

/// A relaunch comes back under the same pane id, and the new program's screen has
/// nothing to do with how recently the old one was serialized.
#[test]
fn a_forgotten_pane_is_due_at_once_like_a_new_one() {
    let now = Instant::now();
    let mut tracker = snapshotted_pane(now);

    tracker.forget(PANE);
    tracker.observe(PANE, ENTER, || (ROWS, COLS));
    tracker.note_owed(PANE, 0);

    assert_eq!(
        tracker.settle_restless(&[PANE], now + TICK).len(),
        1,
        "the new program must not wait out the previous one's window"
    );
}

/// However overdue a pane is, a screen may only be spliced where its stream ends
/// clean; the tick that closes the sequence is the one that takes it.
#[test]
fn a_pane_torn_mid_sequence_waits_for_the_tick_that_closes_it() {
    let now = Instant::now();
    let mut tracker = PaneModeTracker::default();
    tracker.observe(PANE, b"\x1b[?1049h\x1b[", || (ROWS, COLS));
    tracker.note_owed(PANE, 0);
    assert!(
        tracker.settle_restless(&[PANE], now).is_empty(),
        "an open sequence is no place to splice a screen"
    );

    tracker.observe(PANE, b"1;1Hpainted", || (ROWS, COLS));
    assert!(tracker.at_boundary(PANE));
    tracker.note_owed(PANE, 0);
    assert_eq!(
        tracker.settle_restless(&[PANE], now + TICK).len(),
        1,
        "and the next clean tick takes what the torn one deferred"
    );
}

/// A synchronized update still open is not merely a torn sequence: the grid is
/// missing the bytes the update is holding, so a snapshot now would leave them out of
/// the record for good. However overdue the pane is, the pass must wait for the clock
/// to end the update.
#[test]
fn a_pane_holding_a_synchronized_update_is_left_alone() {
    let now = Instant::now();
    let mut tracker = snapshotted_pane(now);

    tracker.observe(PANE, b"\x1b[?2026h\x1b[2Jheld", || (ROWS, COLS));
    assert!(!tracker.screen_current(PANE), "the update holds its bytes");
    tracker.note_owed(PANE, 0);
    assert!(
        tracker.settle_restless(&[PANE], now + WINDOW).is_empty(),
        "a grid missing the update's bytes must never be snapshotted"
    );

    let settled = tracker.settle_sync(now + SYNC_WINDOW);
    assert_eq!(settled.len(), 1, "the clock ends the update");
    assert!(tracker.screen_current(PANE));
    assert_eq!(
        tracker.settle_restless(&[PANE], now + WINDOW).len(),
        1,
        "and the screen it now holds may be taken"
    );
}

/// The pace is per pane: one pane's window must not hold another pane's screen
/// back behind a `since` that keeps growing.
#[test]
fn one_panes_window_does_not_hold_up_another() {
    let now = Instant::now();
    let mut tracker = snapshotted_pane(now);
    let other = PANE + 1;
    tracker.observe(other, ENTER, || (ROWS, COLS));
    tracker.note_owed(other, 0);
    tracker.note_owed(PANE, 0);

    let screens = tracker.settle_restless(&[PANE, other], now + TICK);

    assert_eq!(
        screens.len(),
        1,
        "only the pane whose screen has not been taken is due"
    );
    assert_eq!(screens[0].0, other);
}

/// A pane the tick does not name still gets its owed screen once its window
/// passes: an idle pane is not restless, but it is still owed.
#[test]
fn an_idle_pane_that_owes_bytes_is_settled_without_being_restless() {
    let now = Instant::now();
    let mut tracker = snapshotted_pane(now);

    tracker.note_owed(PANE, 16);
    assert!(
        tracker.settle_restless(&[], now + WINDOW - TICK).is_empty(),
        "an owed pane waits out its window even when the tick names nobody"
    );
    assert_eq!(
        tracker.settle_restless(&[], now + WINDOW).len(),
        1,
        "and the pass takes it once the window has passed"
    );
    assert!(
        tracker.settle_restless(&[], now + 2 * WINDOW).is_empty(),
        "after which it owes nothing and must not be taken again"
    );
}

/// The verdict the worker's loop acts on: an alternate-screen pane is left for the
/// paced pass until a chunk takes its own screen, and a normal-screen pane never is.
#[test]
fn the_worker_leaves_a_pane_restless_only_while_it_owes_a_screen() {
    let dir = tempfile::TempDir::new().unwrap();
    let hub = super::spawn_hub(&dir.path().to_string_lossy(), Vec::new(), Vec::new());
    hub.register_pane(PANE, ROWS, COLS, None, None);
    let now = Instant::now();
    let mut tracker = snapshotted_pane(now);

    assert!(
        hub.record_pane_output(PANE, frame(0), &mut tracker, now),
        "a chunk that does not take its own screen leaves it for the tick"
    );
    let crowded = vec![b'z'; MAX_TERMINAL_SCROLLBACK_BYTES + 1];
    assert!(
        !hub.record_pane_output(PANE, crowded, &mut tracker, now),
        "a crowded chunk takes its own screen"
    );
    assert!(
        tracker
            .settle_restless(&[PANE], now + WINDOW - TICK)
            .is_empty(),
        "and leaves nothing owed, so the paced pass adds nothing before the window"
    );
    assert!(
        !hub.record_pane_output(PANE, b"\x1b[?1049lback".to_vec(), &mut tracker, now),
        "a normal-screen pane is never left for the alternate-screen pass"
    );
    hub.stop();
}

/// Snapshots a busy pane costs per second, with and without the pace. Not a
/// behavioural test and not run with the suite; kept because it is the reason the
/// pace exists, and rerun after changing the pass. Two panes: an ordinary 24x80
/// screen, and the 130x500 truecolour screen a snapshot is most expensive for.
///
/// ```text
/// cargo test --release measure_snapshots_per_second -- --ignored --nocapture
/// ```
#[test]
#[ignore]
fn measure_snapshots_per_second_for_a_busy_pane() {
    // A truecolour image renderer's worst case: every cell written and every cell
    // a different colour.
    fn dense_paint(rows: u16, cols: u16) -> Vec<u8> {
        use std::fmt::Write as _;
        let mut paint = String::from("\x1b[?1049h");
        for row in 0..rows {
            let _ = write!(paint, "\x1b[{};1H", row + 1);
            for col in 0..cols {
                let _ = write!(
                    paint,
                    "\x1b[38;2;{};{};{}mX",
                    row % 255,
                    col % 255,
                    (row + col) % 255
                );
            }
        }
        paint.into_bytes()
    }

    // Two seconds of the worker's tick.
    let ticks = usize::try_from(Duration::from_secs(2).as_millis() / TICK.as_millis()).unwrap();
    for (rows, cols) in [(ROWS, COLS), (130, 500)] {
        let paint = if (rows, cols) == (ROWS, COLS) {
            ENTER.to_vec()
        } else {
            dense_paint(rows, cols)
        };
        let mut measured = Vec::new();
        for paced in [false, true] {
            let dir = tempfile::TempDir::new().unwrap();
            let mut desk = Desk::sized(&dir, rows, cols);
            desk.paced = paced;
            desk.output(&paint);
            desk.tick();
            let started = Instant::now();
            for n in 0..ticks {
                desk.output(&frame(n));
                desk.tick();
            }
            let elapsed = started.elapsed();
            let replay_bytes = assert_replay_matches_live(&desk);
            measured.push((paced, desk.snapshots, elapsed, replay_bytes));
            desk.hub.stop();
        }
        for (paced, snapshots, elapsed, replay_bytes) in measured {
            let label = if paced { "paced" } else { "every tick" };
            eprintln!(
                "{rows}x{cols} {label}: {snapshots} snapshots over {ticks} ticks \
                 ({:.0}/s), {elapsed:?} of work, replay {replay_bytes} bytes",
                snapshots as f64 / 2.0
            );
        }
    }
}
