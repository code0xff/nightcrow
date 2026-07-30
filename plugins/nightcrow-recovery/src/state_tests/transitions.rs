//! Every transition of the machine, and the bound on how many times it will try.

use super::*;

/// The monotonic instant matching `T0 + delta` on the wall clock, so both clocks
/// advance together and no jump is detected.
fn at(base: Instant, delta: i64) -> Instant {
    base + Duration::from_secs(delta as u64)
}

#[test]
fn a_usage_limit_with_a_known_reset_waits_until_that_reset() {
    let mut rec = recovery();
    let out = rec.note_limit(usage(Some(SESSION), Some(RESET)), T0, Instant::now());
    assert_eq!(rec.state(), RecoveryState::WaitingForReset);
    assert_eq!(rec.deadline_epoch(), Some(RESET + RESET_GRACE_SECS));
    assert_eq!(states(&out), vec!["limit_detected", "waiting_for_reset"]);
    assert!(action(&out).is_none(), "a wait asks the host for nothing");
}

#[test]
fn the_same_limit_reported_twice_is_one_episode_and_changes_nothing() {
    let mut rec = recovery();
    let mono = Instant::now();
    rec.note_limit(usage(Some(SESSION), Some(RESET)), T0, mono);
    let again = rec.note_limit(usage(Some(SESSION), Some(RESET)), T0 + 1, mono);
    assert!(again.is_empty());
    assert_eq!(rec.state(), RecoveryState::WaitingForReset);
    assert_eq!(rec.deadline_epoch(), Some(RESET + RESET_GRACE_SECS));
}

#[test]
fn a_limit_naming_a_different_session_starts_a_new_episode() {
    let mut rec = recovery();
    let mono = Instant::now();
    rec.note_limit(usage(Some(SESSION), Some(RESET)), T0, mono);
    let out = rec.note_limit(usage(Some(OTHER_SESSION), Some(RESET + 60)), T0, mono);
    assert_eq!(rec.session_id(), Some(OTHER_SESSION));
    assert_eq!(rec.deadline_epoch(), Some(RESET + 60 + RESET_GRACE_SECS));
    assert!(!out.is_empty());
}

#[test]
fn a_transient_failure_backs_off_instead_of_waiting_for_a_usage_window() {
    let mut rec = recovery();
    let out = rec.note_limit(transient(), T0, Instant::now());
    assert_eq!(rec.state(), RecoveryState::Backoff);
    assert_eq!(rec.deadline_epoch(), Some(T0 + BACKOFF_BASE_SECS));
    assert_eq!(states(&out), vec!["limit_detected", "backoff"]);
}

#[test]
fn a_limit_with_no_known_reset_backs_off() {
    let mut rec = recovery();
    rec.note_limit(usage(Some(SESSION), None), T0, Instant::now());
    assert_eq!(rec.state(), RecoveryState::Backoff);
    assert_eq!(rec.deadline_epoch(), Some(T0 + BACKOFF_BASE_SECS));
}

#[test]
fn a_failure_only_a_human_can_fix_goes_straight_to_needs_attention() {
    let mut rec = recovery();
    let out = rec.note_limit(needs_human(), T0, Instant::now());
    assert_eq!(rec.state(), RecoveryState::NeedsAttention);
    assert_eq!(states(&out), vec!["limit_detected", "needs_attention"]);
    assert!(action(&out).is_none());
}

#[test]
fn needs_attention_ignores_every_later_limit_report() {
    let mut rec = recovery();
    let mono = Instant::now();
    rec.note_limit(needs_human(), T0, mono);
    let out = rec.note_limit(usage(Some(SESSION), Some(RESET)), T0, mono);
    assert!(out.is_empty());
    assert_eq!(rec.state(), RecoveryState::NeedsAttention);
}

#[test]
fn a_stale_generation_cannot_drive_a_transition() {
    let mut rec = recovery();
    let mono = Instant::now();
    rec.on_event(&opened(2)).expect("a newer generation");
    rec.note_limit(usage(Some(SESSION), Some(RESET)), T0, mono);
    assert_eq!(rec.state(), RecoveryState::WaitingForReset);

    for stale in [exited(1), closed(1), user_input(1), went_idle(1)] {
        assert!(rec.on_event(&stale).is_none(), "{stale:?} is stale");
        assert_eq!(rec.state(), RecoveryState::WaitingForReset);
        assert_eq!(rec.generation(), 2);
        assert!(rec.alive(), "a stale exit does not mark the pane dead");
    }
}

#[test]
fn a_clock_pushed_forward_during_a_wait_does_not_resume_early() {
    let mut rec = recovery();
    let provider = FakeProvider::relaunch_only();
    let mono = Instant::now();
    rec.note_limit(usage(Some(SESSION), Some(RESET)), T0, mono);
    rec.on_event(&exited(1)).expect("current generation");
    // A minute of real time, then the wall clock leaps a day ahead.
    tick_at(&mut rec, &provider, T0 + 60, at(mono, 60));
    let out = tick_at(&mut rec, &provider, T0 + 60 + 86_400, at(mono, 61));
    assert_eq!(rec.state(), RecoveryState::WaitingForReset);
    assert!(action(&out).is_none());
}

#[test]
fn every_state_reports_the_name_the_host_displays() {
    for (state, name) in [
        (RecoveryState::Idle, "idle"),
        (RecoveryState::LimitDetected, "limit_detected"),
        (RecoveryState::WaitingForReset, "waiting_for_reset"),
        (RecoveryState::ReadyToResume, "ready_to_resume"),
        (RecoveryState::Resuming, "resuming"),
        (RecoveryState::Backoff, "backoff"),
        (RecoveryState::NeedsAttention, "needs_attention"),
    ] {
        assert_eq!(state.as_str(), name);
    }
}

#[test]
fn a_status_carries_the_pane_identity_the_deadline_and_the_attempt() {
    let mut rec = recovery();
    let out = rec.note_limit(usage(Some(SESSION), Some(RESET)), T0, Instant::now());
    let last = out.last().expect("a status was reported");
    match last {
        PluginCommand::Status {
            v,
            token,
            generation,
            state,
            deadline_epoch,
            attempt,
            ..
        } => {
            assert_eq!(*v, PROTOCOL_VERSION);
            assert_eq!(token, TOKEN);
            assert_eq!(*generation, 1);
            assert_eq!(state, "waiting_for_reset");
            assert_eq!(*deadline_epoch, Some(RESET + RESET_GRACE_SECS));
            assert_eq!(*attempt, 0);
        }
        other => panic!("expected a status, got {other:?}"),
    }
}
