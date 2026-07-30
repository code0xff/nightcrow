use super::common::state_with_event_queue;
use crate::backend::BackendEvent;
use crate::runtime::terminal::recovery::RECOVERY_CANCELLED;

const STATE: &str = "waiting_for_reset";
const DEADLINE: i64 = 1_700_000_000;

fn report(pane: crate::backend::PaneId, state: &str, attempt: u32) -> BackendEvent {
    BackendEvent::Recovery {
        pane,
        state: state.to_string(),
        detail: Some("provider window closed".to_string()),
        deadline_epoch: Some(DEADLINE),
        attempt,
    }
}

#[test]
fn a_reported_state_is_kept_verbatim_for_the_pane_it_names() {
    let (mut state, events) = state_with_event_queue();
    state.create_pane_now().unwrap();
    let pane = state.panes[0].id;

    events.borrow_mut().push(report(pane, STATE, 2));
    state.poll();

    let held = state.recovery_for(pane).expect("the report was dropped");
    assert_eq!(held.state, STATE);
    assert_eq!(held.deadline_epoch, Some(DEADLINE));
    assert_eq!(held.attempt, 2);
    assert_eq!(held.detail.as_deref(), Some("provider window closed"));
}

#[test]
fn a_later_report_replaces_the_earlier_one_rather_than_accumulating() {
    let (mut state, events) = state_with_event_queue();
    state.create_pane_now().unwrap();
    let pane = state.panes[0].id;

    events.borrow_mut().push(report(pane, STATE, 1));
    state.poll();
    events.borrow_mut().push(report(pane, "backoff", 3));
    state.poll();

    let held = state.recovery_for(pane).expect("the report was dropped");
    assert_eq!(held.state, "backoff");
    assert_eq!(held.attempt, 3, "the newest report is the whole picture");
}

#[test]
fn a_cancelled_report_clears_the_pane_instead_of_being_stored() {
    let (mut state, events) = state_with_event_queue();
    state.create_pane_now().unwrap();
    let pane = state.panes[0].id;

    events.borrow_mut().push(report(pane, STATE, 1));
    state.poll();
    events
        .borrow_mut()
        .push(report(pane, RECOVERY_CANCELLED, 0));
    state.poll();

    assert!(
        state.recovery_for(pane).is_none(),
        "a finished recovery must leave no badge behind"
    );
    assert!(!state.can_cancel_recovery());
}

#[test]
fn a_report_survives_the_pane_it_names_going_away() {
    // The report that matters most arrives while the pane is gone and its slot is
    // held for a relaunch, so an exit must not take it with it.
    let (mut state, events) = state_with_event_queue();
    state.create_pane_now().unwrap();
    let pane = state.panes[0].id;

    events.borrow_mut().push(report(pane, STATE, 1));
    events.borrow_mut().push(BackendEvent::Exited { pane });
    state.poll();

    assert!(state.panes.is_empty(), "the pane must be gone");
    assert_eq!(
        state.recovery_focus().map(|(id, _)| id),
        Some(pane),
        "a pane with no tab must still be reachable"
    );
}

#[test]
fn the_focused_pane_outranks_a_report_for_a_pane_that_is_gone() {
    let (mut state, events) = state_with_event_queue();
    state.create_pane_now().unwrap();
    state.create_pane_now().unwrap();
    let (first, second) = (state.panes[0].id, state.panes[1].id);
    state.active = 1;

    events.borrow_mut().push(report(first, STATE, 1));
    events.borrow_mut().push(report(second, "resuming", 0));
    events
        .borrow_mut()
        .push(BackendEvent::Exited { pane: first });
    state.poll();
    state.active = 0;

    // `poll` removed the exited pane, so the surviving one is index 0 now.
    assert_eq!(state.panes.len(), 1);
    assert_eq!(
        state.recovery_focus().map(|(id, _)| id),
        Some(second),
        "the pane a person is looking at comes first"
    );
}

#[test]
fn cancelling_with_nothing_reported_asks_the_session_for_nothing() {
    let (mut state, _events) = state_with_event_queue();
    state.create_pane_now().unwrap();

    state.cancel_recovery();
    state.poll();

    assert!(
        state.recovery_focus().is_none(),
        "there was nothing to cancel and nothing to show"
    );
}

#[test]
fn cancelling_asks_the_session_and_clears_only_on_its_answer() {
    let (mut state, events) = state_with_event_queue();
    state.create_pane_now().unwrap();
    let pane = state.panes[0].id;
    events.borrow_mut().push(report(pane, STATE, 1));
    state.poll();

    state.cancel_recovery();

    // Nothing is assumed locally: the badge is still there until the session
    // confirms, which is what keeps a refused cancellation visible.
    assert!(state.recovery_for(pane).is_some());
    state.poll();
    assert!(
        state.recovery_for(pane).is_none(),
        "the session's answer must clear the pane"
    );
}
