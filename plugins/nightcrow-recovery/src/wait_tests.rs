use super::*;

/// A fixed "now" so a test's arithmetic is readable. 2026-01-01T00:00:00Z.
const T0: i64 = 1_767_225_600;

fn start() -> Instant {
    Instant::now()
}

fn after(base: Instant, secs: u64) -> Instant {
    base + Duration::from_secs(secs)
}

#[test]
fn a_wait_ends_only_once_both_clocks_have_passed_the_deadline() {
    let mono = start();
    let mut wait = ResetWait::until(T0 + 600, T0, mono);
    let planned = 600 + RESET_GRACE_SECS;
    assert!(!wait.poll(T0 + planned - 1, after(mono, planned as u64 - 1)));
    assert!(wait.poll(T0 + planned, after(mono, planned as u64)));
}

#[test]
fn a_reset_time_already_in_the_past_still_waits_the_floor() {
    let mono = start();
    let mut wait = ResetWait::until(T0 - 10_000, T0, mono);
    assert_eq!(wait.deadline_epoch(), T0 + MIN_WAIT_SECS);
    assert!(!wait.poll(
        T0 + MIN_WAIT_SECS - 1,
        after(mono, MIN_WAIT_SECS as u64 - 1)
    ));
    assert!(wait.poll(T0 + MIN_WAIT_SECS, after(mono, MIN_WAIT_SECS as u64)));
}

#[test]
fn a_reset_time_beyond_the_maximum_wait_is_clamped() {
    let mono = start();
    let wait = ResetWait::until(T0 + MAX_WAIT_SECS * 10, T0, mono);
    assert_eq!(wait.deadline_epoch(), T0 + MAX_WAIT_SECS);
}

#[test]
fn a_wait_ends_a_grace_period_after_the_reported_reset() {
    let wait = ResetWait::until(T0 + 600, T0, start());
    assert_eq!(wait.deadline_epoch(), T0 + 600 + RESET_GRACE_SECS);
}

#[test]
fn a_wall_clock_jumped_forward_does_not_fire_a_wait_early() {
    let mono = start();
    let mut wait = ResetWait::until(T0 + 3600, T0, mono);
    // One minute of real time passes, then the wall clock leaps two hours ahead.
    assert!(!wait.poll(T0 + 60, after(mono, 60)));
    assert!(!wait.poll(T0 + 60 + 7200, after(mono, 61)));
    // Even far past the original wall-clock deadline, the countdown has not run.
    assert!(!wait.poll(T0 + 60 + 7200 + 5, after(mono, 66)));
}

#[test]
fn a_wall_clock_jumped_backwards_does_not_strand_a_wait() {
    let mono = start();
    let planned = 600 + RESET_GRACE_SECS;
    let mut wait = ResetWait::until(T0 + 600, T0, mono);
    // The clock is corrected back by a day after a minute of real time.
    assert!(!wait.poll(T0 + 60, after(mono, 60)));
    let shifted = T0 + 60 - 86_400;
    assert!(!wait.poll(shifted, after(mono, 61)));
    // The wait still ends after its planned real duration, on the new clock.
    let end_mono = after(mono, planned as u64 + 1);
    assert!(wait.poll(shifted + planned - 60, end_mono));
}

#[test]
fn small_clock_jitter_does_not_shift_the_deadline() {
    let mono = start();
    let mut wait = ResetWait::until(T0 + 600, T0, mono);
    let before = wait.deadline_epoch();
    // A second of skew is rounding, not a jump.
    assert!(!wait.poll(T0 + 61, after(mono, 60)));
    assert_eq!(wait.deadline_epoch(), before);
}

#[test]
fn backoff_doubles_each_attempt_and_stops_at_the_cap() {
    let mono = start();
    let step = |attempt| ResetWait::backoff(attempt, T0, mono).deadline_epoch() - T0;
    assert_eq!(step(1), BACKOFF_BASE_SECS);
    assert_eq!(step(2), BACKOFF_BASE_SECS * 2);
    assert_eq!(step(3), BACKOFF_BASE_SECS * 4);
    assert_eq!(step(20), BACKOFF_MAX_SECS);
    assert_eq!(step(u32::MAX), BACKOFF_MAX_SECS);
}

#[test]
fn a_backoff_wait_ends_after_its_own_duration() {
    let mono = start();
    let mut wait = ResetWait::backoff(1, T0, mono);
    let secs = BACKOFF_BASE_SECS as u64;
    assert!(!wait.poll(T0 + secs as i64 - 1, after(mono, secs - 1)));
    assert!(wait.poll(T0 + secs as i64, after(mono, secs)));
}

#[test]
fn now_epoch_reports_a_plausible_current_time() {
    // Any real clock is past 2020 and this test's own compile date.
    assert!(now_epoch() > 1_577_836_800);
}
