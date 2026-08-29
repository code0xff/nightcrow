//! The three ways a recovery is called off, and what each one leaves behind.

use super::*;

#[test]
fn a_human_typing_into_the_pane_cancels_the_wait() {
    let mut rec = recovery();
    rec.note_limit(usage(Some(SESSION), Some(RESET)), T0, Instant::now());
    let out = rec.on_event(&user_input(1)).expect("current generation");
    assert_eq!(rec.state(), RecoveryState::Idle);
    assert_eq!(rec.deadline_epoch(), None);
    assert_eq!(rec.session_id(), None);
    assert_eq!(states(&out), vec!["idle"]);
}

#[test]
fn the_pane_slot_closing_cancels_the_wait() {
    let mut rec = recovery();
    rec.note_limit(usage(Some(SESSION), Some(RESET)), T0, Instant::now());
    let out = rec.on_event(&closed(1)).expect("current generation");
    assert_eq!(rec.state(), RecoveryState::Idle);
    assert_eq!(states(&out), vec!["idle"]);
}

#[test]
fn a_respawn_the_plugin_did_not_ask_for_cancels_the_wait() {
    let mut rec = recovery();
    rec.note_limit(usage(Some(SESSION), Some(RESET)), T0, Instant::now());
    let out = rec.on_event(&opened(2)).expect("a newer generation");
    assert_eq!(rec.state(), RecoveryState::Idle);
    assert_eq!(rec.generation(), 2);
    assert_eq!(rec.deadline_epoch(), None);
    assert_eq!(states(&out), vec!["idle"]);
}

#[test]
fn cancelling_an_already_idle_pane_reports_nothing() {
    let mut rec = recovery();
    let first = rec.on_event(&user_input(1)).expect("current generation");
    let second = rec.on_event(&user_input(1)).expect("current generation");
    assert!(first.is_empty(), "the pane was already idle");
    assert!(second.is_empty(), "and cancelling again is the same no-op");
    assert_eq!(rec.state(), RecoveryState::Idle);
}

#[test]
fn cancelling_the_same_wait_twice_reports_the_change_once() {
    let mut rec = recovery();
    rec.note_limit(usage(Some(SESSION), Some(RESET)), T0, Instant::now());
    let first = rec.on_event(&user_input(1)).expect("current generation");
    let second = rec.on_event(&user_input(1)).expect("current generation");
    assert_eq!(states(&first), vec!["idle"]);
    assert!(second.is_empty());
}

#[test]
fn a_human_taking_the_pane_back_refunds_the_attempt_budget() {
    let mut rec = recovery();
    let provider = FakeProvider::relaunch_only();
    let mono = Instant::now();
    rec.on_event(&exited(1)).expect("current generation");
    rec.note_limit(usage(Some(SESSION), None), T0, mono);
    let ready = BACKOFF_BASE_SECS;
    tick_at(
        &mut rec,
        &provider,
        T0 + ready,
        mono + Duration::from_secs(ready as u64),
    );
    assert_eq!(rec.attempt(), 1);
    rec.on_event(&user_input(2)).expect("a newer generation");
    assert_eq!(rec.attempt(), 0);
    assert_eq!(rec.state(), RecoveryState::Idle);
}

#[test]
fn a_cancelled_pane_can_start_a_new_episode() {
    let mut rec = recovery();
    let mono = Instant::now();
    rec.note_limit(usage(Some(SESSION), Some(RESET)), T0, mono);
    rec.on_event(&user_input(1)).expect("current generation");
    let out = rec.note_limit(usage(Some(SESSION), Some(RESET)), T0 + 10, mono);
    assert_eq!(rec.state(), RecoveryState::WaitingForReset);
    assert_eq!(states(&out), vec!["limit_detected", "waiting_for_reset"]);
}

#[test]
fn two_panes_in_one_repo_recover_their_own_sessions_independently() {
    let mut first = PaneRecovery::new(TOKEN.to_string(), 1);
    let mut second = PaneRecovery::new(OTHER_TOKEN.to_string(), 1);
    let mono = Instant::now();
    let out = first.note_limit(usage(Some(SESSION), Some(RESET)), T0, mono);
    second.note_limit(usage(Some(OTHER_SESSION), Some(RESET + 600)), T0, mono);

    assert_eq!(first.session_id(), Some(SESSION));
    assert_eq!(second.session_id(), Some(OTHER_SESSION));
    assert_eq!(first.deadline_epoch(), Some(RESET + RESET_GRACE_SECS));
    assert_eq!(
        second.deadline_epoch(),
        Some(RESET + 600 + RESET_GRACE_SECS)
    );

    // A status names the pane it belongs to, which is how the host tells them
    // apart when both panes run in the same repository.
    match out.last().expect("a status") {
        PluginCommand::Status { token, .. } => assert_eq!(token, TOKEN),
        other => panic!("expected a status, got {other:?}"),
    }

    // Cancelling one leaves the other alone.
    first.on_event(&user_input(1)).expect("current generation");
    assert_eq!(first.state(), RecoveryState::Idle);
    assert_eq!(second.state(), RecoveryState::WaitingForReset);
}

#[test]
fn a_shutdown_is_not_a_pane_event_and_moves_nothing() {
    let mut rec = recovery();
    rec.note_limit(usage(Some(SESSION), Some(RESET)), T0, Instant::now());
    let out = rec.on_event(&PluginEvent::Shutdown {
        v: PROTOCOL_VERSION,
    });
    assert!(out.is_none(), "a shutdown names no pane");
    assert_eq!(rec.state(), RecoveryState::WaitingForReset);
}

#[test]
fn the_wait_is_dropped_when_a_cancelled_pane_reaches_a_resume() {
    let mut rec = recovery();
    let provider = FakeProvider::relaunch_only();
    let mono = Instant::now();
    rec.on_event(&exited(1)).expect("current generation");
    rec.note_limit(usage(Some(SESSION), Some(RESET)), T0, mono);
    // The wait ends, but the episode was cancelled in between, so there is
    // nothing left to resume and nothing is asked of the host.
    rec.on_event(&user_input(1)).expect("current generation");
    let elapsed = RESET - T0 + RESET_GRACE_SECS;
    let out = tick_at(
        &mut rec,
        &provider,
        T0 + elapsed,
        mono + Duration::from_secs(elapsed as u64),
    );
    assert!(out.is_empty());
    assert_eq!(rec.state(), RecoveryState::Idle);
    assert_eq!(rec.attempt(), 0);
}
