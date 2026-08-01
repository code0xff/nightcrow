//! The search caret's blink phase. Pure arithmetic on elapsed time, so it can
//! be pinned without a clock or a frame.

use crate::ui::helpers::{CARET_BLINK, caret_lit};
use std::time::Duration;

#[test]
fn the_caret_starts_lit_and_stays_lit_for_half_the_period() {
    let half = CARET_BLINK / 2;
    assert!(caret_lit(Duration::ZERO));
    assert!(caret_lit(half - Duration::from_millis(1)));
}

#[test]
fn the_caret_goes_dark_for_the_other_half() {
    let half = CARET_BLINK / 2;
    assert!(!caret_lit(half));
    assert!(!caret_lit(CARET_BLINK - Duration::from_millis(1)));
}

#[test]
fn the_phase_repeats_every_period() {
    // A caret that drifted out of its cycle would eventually sit still, which
    // is the bug this replaces.
    for cycle in 0..5 {
        let base = CARET_BLINK * cycle;
        assert!(caret_lit(base), "cycle {cycle} must start lit");
        assert!(!caret_lit(base + CARET_BLINK / 2), "cycle {cycle} half");
    }
}
