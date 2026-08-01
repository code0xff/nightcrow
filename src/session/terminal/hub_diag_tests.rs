//! The arrival log's arithmetic: what one frame of input amounts to, and how a
//! run of them is counted without filling the log.

use super::hub_diag::{BURST_GAP, ClearWatch, MAX_LINES_PER_BURST};
use std::time::{Duration, Instant};

const PANE: crate::backend::PaneId = 1;

#[test]
fn ordinary_input_is_not_noted() {
    let mut watch = ClearWatch::default();
    assert!(watch.record(PANE, b"ls -la\r", Instant::now()).is_none());
}

#[test]
fn a_clear_byte_is_noted_with_what_rode_with_it() {
    let mut watch = ClearWatch::default();
    let now = Instant::now();

    // A keystroke arrives alone; this one did not.
    let note = watch.record(PANE, b"\x0cabc", now).expect("not noted");

    assert_eq!(note.clears, 1);
    assert_eq!(note.other_bytes, 3);
    assert_eq!(note.in_burst, 1);
    assert!(!note.suppressed);
}

#[test]
fn repeats_inside_the_window_count_as_one_run() {
    let mut watch = ClearWatch::default();
    let start = Instant::now();

    let first = watch.record(PANE, b"\x0c", start).expect("not noted");
    let second = watch
        .record(PANE, b"\x0c", start + Duration::from_millis(200))
        .expect("not noted");

    assert_eq!(first.in_burst, 1);
    assert_eq!(second.in_burst, 2);
    assert_eq!(second.gap_ms, 200);
    assert_eq!(second.previous_burst_total, None);
}

#[test]
fn a_quiet_gap_starts_a_new_run() {
    let mut watch = ClearWatch::default();
    let start = Instant::now();
    watch.record(PANE, b"\x0c", start);

    let later = watch
        .record(PANE, b"\x0c", start + BURST_GAP + Duration::from_millis(1))
        .expect("not noted");

    assert_eq!(later.in_burst, 1, "the count restarts with the run");
}

#[test]
fn a_long_run_stops_writing_lines_but_keeps_counting() {
    let mut watch = ClearWatch::default();
    let start = Instant::now();
    let mut at = start;

    let budget = MAX_LINES_PER_BURST;
    for _ in 0..budget {
        let note = watch.record(PANE, b"\x0c", at).expect("not noted");
        assert!(!note.suppressed, "inside the budget nothing is suppressed");
        at += Duration::from_millis(30);
    }

    let over = watch.record(PANE, b"\x0c", at).expect("not noted");
    assert!(over.suppressed, "past the budget the line is dropped");
    assert_eq!(
        over.in_burst,
        budget + 1,
        "but the run's total must still be right"
    );
}

#[test]
fn the_run_that_outgrew_the_budget_reports_its_total() {
    // The point of the budget is that the log stays readable; the point of this
    // is that it never lies about how much it left out.
    let mut watch = ClearWatch::default();
    let start = Instant::now();
    let mut at = start;
    for _ in 0..MAX_LINES_PER_BURST + 5 {
        watch.record(PANE, b"\x0c", at);
        at += Duration::from_millis(30);
    }

    let after_quiet = watch
        .record(PANE, b"\x0c", at + BURST_GAP + Duration::from_millis(1))
        .expect("not noted");

    assert_eq!(
        after_quiet.previous_burst_total,
        Some(MAX_LINES_PER_BURST + 5)
    );
}

#[test]
fn panes_are_counted_apart() {
    let mut watch = ClearWatch::default();
    let now = Instant::now();
    watch.record(PANE, b"\x0c", now);

    let other = watch.record(PANE + 1, b"\x0c", now).expect("not noted");

    assert_eq!(other.in_burst, 1);
}

#[test]
fn a_gone_pane_is_forgotten() {
    let mut watch = ClearWatch::default();
    let now = Instant::now();
    watch.record(PANE, b"\x0c", now);

    watch.forget(PANE);

    let after = watch
        .record(PANE, b"\x0c", now + Duration::from_millis(10))
        .expect("not noted");
    assert_eq!(after.in_burst, 1, "a reused pane id starts clean");
}
