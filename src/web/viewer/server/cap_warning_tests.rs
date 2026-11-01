//! One line per burst, and the count the line accounts for.

use super::{CapWarning, WARN_INTERVAL};
use std::time::{Duration, Instant};

#[test]
fn the_first_refusal_is_always_reported() {
    let start = Instant::now();
    let mut warning = CapWarning::new();
    assert_eq!(warning.hit(start), Some(1));
}

#[test]
fn a_burst_of_refusals_is_one_warning() {
    let start = Instant::now();
    let mut warning = CapWarning::new();
    assert_eq!(warning.hit(start), Some(1));

    for i in 1..1_000 {
        assert_eq!(
            warning.hit(start + Duration::from_millis(i)),
            None,
            "refusal {i} must stay inside the burst"
        );
    }

    assert_eq!(
        warning.hit(start + WARN_INTERVAL),
        Some(1_000),
        "the next warning must account for everything it silenced"
    );
}

#[test]
fn a_later_burst_is_reported_again() {
    let start = Instant::now();
    let mut warning = CapWarning::new();
    warning.hit(start);
    assert_eq!(warning.hit(start + Duration::from_secs(1)), None);
    assert_eq!(warning.hit(start + WARN_INTERVAL), Some(2));
}

/// Nothing is dropped from the count by a long quiet spell: the line after a
/// gap still says how many connections were refused since the last one.
#[test]
fn the_count_survives_a_quiet_spell() {
    let start = Instant::now();
    let mut warning = CapWarning::new();
    warning.hit(start);
    warning.hit(start + Duration::from_millis(500));
    assert_eq!(
        warning.hit(start + Duration::from_secs(60)),
        Some(2),
        "both refusals since the last line must be counted"
    );
}
