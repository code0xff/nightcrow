//! The two guards on a client's diagnostic note: what it may put in the log, and
//! how often.
//!
//! Both exist because the note comes from the page, and a scripted page is the
//! very thing being investigated — it must not be able to write the log at will
//! or choose what appears in it.

use super::session::{ReportBudget, sanitized_code};
use std::time::{Duration, Instant};

#[test]
fn a_key_code_survives_intact() {
    assert_eq!(sanitized_code("KeyL"), "KeyL");
    assert_eq!(sanitized_code("F12"), "F12");
}

#[test]
fn anything_that_is_not_a_key_code_is_stripped() {
    // Newlines would forge a log line; the rest simply has no business here.
    assert_eq!(sanitized_code("Key L\nviewer: forged"), "KeyLviewerforged");
    assert_eq!(sanitized_code("\u{1b}[31mred"), "31mred");
}

#[test]
fn an_overlong_code_is_cut() {
    let cut = sanitized_code(&"a".repeat(500));
    assert_eq!(cut.len(), 16);
}

#[test]
fn an_empty_code_says_so_rather_than_logging_nothing() {
    assert_eq!(sanitized_code(""), "unknown");
    assert_eq!(sanitized_code("\u{0}\u{0}"), "unknown");
}

#[test]
fn a_burst_of_reports_fits_in_the_budget() {
    // The event under investigation is tens of notes over a few seconds. If the
    // budget could not hold that, it would censor the evidence.
    let start = Instant::now();
    let mut budget = ReportBudget::new(start);

    for i in 0..60 {
        assert!(
            budget.allow(start + Duration::from_millis(i * 100)),
            "note {i} must fit"
        );
    }
}

#[test]
fn a_client_that_keeps_talking_is_cut_off() {
    let start = Instant::now();
    let mut budget = ReportBudget::new(start);
    while budget.allow(start) {}

    assert!(!budget.allow(start), "the window is spent");
}

#[test]
fn the_budget_returns_with_the_next_window() {
    let start = Instant::now();
    let mut budget = ReportBudget::new(start);
    while budget.allow(start) {}

    assert!(budget.allow(start + Duration::from_secs(61)));
}
