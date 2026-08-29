//! What the machine asks of the host once a wait is over, and the bound on how
//! often it will ask.

use super::*;
use crate::wait::BACKOFF_MAX_SECS;

/// The monotonic instant matching `T0 + delta` on the wall clock, so both clocks
/// advance together and no jump is detected.
fn at(base: Instant, delta: i64) -> Instant {
    base + Duration::from_secs(delta as u64)
}

/// Wall seconds from `T0` at which a wait for [`RESET`] is over.
const AFTER_RESET: i64 = RESET - T0 + RESET_GRACE_SECS;

#[test]
fn a_wait_that_ends_relaunches_a_pane_whose_process_is_gone() {
    let mut rec = recovery();
    let provider = FakeProvider::relaunch_only();
    let mono = Instant::now();
    rec.note_limit(usage(Some(SESSION), Some(RESET)), T0, mono);
    rec.on_event(&exited(1)).expect("current generation");
    let out = tick_at(&mut rec, &provider, T0 + AFTER_RESET, at(mono, AFTER_RESET));
    assert_eq!(rec.state(), RecoveryState::Resuming);
    assert_eq!(rec.attempt(), 1);
    assert_eq!(states(&out), vec!["ready_to_resume", "resuming"]);
    match action(&out) {
        Some(PluginCommand::Relaunch { resume_args, .. }) => {
            assert_eq!(resume_args, &["--resume".to_string(), SESSION.to_string()]);
        }
        other => panic!("expected a relaunch, got {other:?}"),
    }
}

#[test]
fn a_pane_whose_process_is_still_running_is_not_relaunched() {
    let mut rec = recovery();
    let provider = FakeProvider {
        alive_plan: Some(ResumePlan::Relaunch(vec![
            "--resume".to_string(),
            SESSION.to_string(),
        ])),
        ..FakeProvider::default()
    };
    let mono = Instant::now();
    rec.note_limit(usage(Some(SESSION), Some(RESET)), T0, mono);
    let out = tick_at(&mut rec, &provider, T0 + AFTER_RESET, at(mono, AFTER_RESET));
    assert_eq!(rec.state(), RecoveryState::ReadyToResume);
    assert!(action(&out).is_none(), "nothing is asked of a live pane");
}

#[test]
fn a_relaunch_that_lands_as_a_new_generation_confirms_the_resume() {
    let mut rec = recovery();
    let provider = FakeProvider::relaunch_only();
    let mono = Instant::now();
    rec.note_limit(usage(Some(SESSION), Some(RESET)), T0, mono);
    rec.on_event(&exited(1)).expect("current generation");
    tick_at(&mut rec, &provider, T0 + AFTER_RESET, at(mono, AFTER_RESET));
    let out = rec.on_event(&opened(2)).expect("a newer generation");
    assert_eq!(rec.state(), RecoveryState::Idle);
    assert_eq!(rec.generation(), 2);
    assert_eq!(rec.attempt(), 0, "a resume that landed refunds the budget");
    assert_eq!(states(&out), vec!["idle"]);
}

#[test]
fn a_resume_showing_no_sign_of_life_backs_off_and_tries_again() {
    let mut rec = recovery();
    let provider = FakeProvider::relaunch_only();
    let mono = Instant::now();
    rec.on_event(&exited(1)).expect("current generation");
    rec.note_limit(usage(Some(SESSION), None), T0, mono);
    let ready = BACKOFF_BASE_SECS;
    tick_at(&mut rec, &provider, T0 + ready, at(mono, ready));
    assert_eq!(rec.state(), RecoveryState::Resuming);

    let gave_up = ready + RESUME_CONFIRM_SECS as i64;
    tick_at(&mut rec, &provider, T0 + gave_up, at(mono, gave_up));
    assert_eq!(rec.state(), RecoveryState::Backoff);
    assert_eq!(rec.attempt(), 1);
    assert_eq!(
        rec.deadline_epoch(),
        Some(T0 + gave_up + BACKOFF_BASE_SECS * 2)
    );
}

#[test]
fn attempts_stop_at_the_maximum_and_land_in_needs_attention() {
    let mut rec = recovery();
    let provider = FakeProvider::relaunch_only();
    let mono = Instant::now();
    rec.on_event(&exited(1)).expect("current generation");
    rec.note_limit(usage(Some(SESSION), None), T0, mono);
    let mut elapsed = 0i64;
    for attempt in 1..=MAX_RESUME_ATTEMPTS {
        // Past any backoff step, so the wait is certainly over.
        elapsed += BACKOFF_MAX_SECS;
        let out = tick_at(&mut rec, &provider, T0 + elapsed, at(mono, elapsed));
        assert_eq!(rec.state(), RecoveryState::Resuming, "attempt {attempt}");
        assert_eq!(rec.attempt(), attempt);
        assert!(matches!(action(&out), Some(PluginCommand::Relaunch { .. })));
        elapsed += RESUME_CONFIRM_SECS as i64;
        tick_at(&mut rec, &provider, T0 + elapsed, at(mono, elapsed));
    }
    assert_eq!(rec.state(), RecoveryState::NeedsAttention);
    assert_eq!(rec.attempt(), MAX_RESUME_ATTEMPTS);

    // And it stays there rather than trying a fifth time.
    elapsed += BACKOFF_MAX_SECS;
    let out = tick_at(&mut rec, &provider, T0 + elapsed, at(mono, elapsed));
    assert!(out.is_empty());
    assert_eq!(rec.state(), RecoveryState::NeedsAttention);
}

#[test]
fn an_adapter_that_holds_hands_the_pane_to_its_human() {
    let mut rec = recovery();
    let provider = FakeProvider {
        exited_plan: Some(ResumePlan::Hold("no session id")),
        ..FakeProvider::default()
    };
    let mono = Instant::now();
    rec.note_limit(usage(None, Some(RESET)), T0, mono);
    rec.on_event(&exited(1)).expect("current generation");
    let out = tick_at(&mut rec, &provider, T0 + AFTER_RESET, at(mono, AFTER_RESET));
    assert_eq!(rec.state(), RecoveryState::NeedsAttention);
    assert!(action(&out).is_none());
    assert_eq!(rec.attempt(), 0, "a hold is not an attempt");
}

#[test]
fn an_adapter_offering_unsafe_resume_args_is_refused_without_asking_the_host() {
    let mut rec = recovery();
    let provider = FakeProvider {
        exited_plan: Some(ResumePlan::Relaunch(vec![
            "--resume".to_string(),
            "abc; rm -rf /".to_string(),
        ])),
        ..FakeProvider::default()
    };
    let mono = Instant::now();
    rec.note_limit(usage(Some(SESSION), Some(RESET)), T0, mono);
    rec.on_event(&exited(1)).expect("current generation");
    let out = tick_at(&mut rec, &provider, T0 + AFTER_RESET, at(mono, AFTER_RESET));
    assert_eq!(rec.state(), RecoveryState::NeedsAttention);
    assert!(action(&out).is_none());
}

/// A pane the host launched no command in. The state machine still rejects a
/// relaunch even though the bundled plugin no longer adopts such panes.
fn bare_shell() -> PaneContext {
    PaneContext {
        command: None,
        ..ctx()
    }
}

#[test]
fn a_pane_the_host_launched_no_command_in_is_never_relaunched() {
    // Putting a process back would start the shell again, not the session, so the
    // host refuses it outright — and no amount of waiting changes that, which is
    // why this ends in needs_attention instead of another backoff.
    let mut rec = recovery();
    let provider = FakeProvider::relaunch_only();
    let mono = Instant::now();
    rec.note_limit(usage(Some(SESSION), Some(RESET)), T0, mono);
    rec.on_event(&exited(1)).expect("current generation");
    let out = rec.tick(
        &provider,
        &bare_shell(),
        T0 + AFTER_RESET,
        at(mono, AFTER_RESET),
    );
    assert_eq!(rec.state(), RecoveryState::NeedsAttention);
    assert!(action(&out).is_none(), "the host is asked for nothing");
    assert_eq!(rec.attempt(), 0, "and no attempt is spent learning that");
}

#[test]
fn an_adapter_with_nothing_to_say_costs_one_attempt_and_backs_off() {
    let mut rec = recovery();
    let provider = FakeProvider {
        exited_plan: None,
        ..FakeProvider::default()
    };
    let mono = Instant::now();
    rec.note_limit(usage(Some(SESSION), Some(RESET)), T0, mono);
    rec.on_event(&exited(1)).expect("current generation");
    tick_at(&mut rec, &provider, T0 + AFTER_RESET, at(mono, AFTER_RESET));
    assert_eq!(rec.state(), RecoveryState::Backoff);
}
