//! Tests for panes created by another client in a shared session.

use super::common::*;
use crate::backend::BackendEvent;

#[test]
fn a_pane_this_client_asked_for_takes_the_focus() {
    let (mut state, _events) = state_with_event_queue();
    state.create_pane_now().unwrap();
    state.create_pane_now().unwrap();

    assert_eq!(state.panes.len(), 2);
    assert_eq!(state.active, 1, "the pane just opened is the active one");
}

#[test]
fn a_pane_someone_else_opened_appears_without_stealing_the_focus() {
    // Which pane a client is looking at is its own business. A pane opened
    // in a browser tab must show up in the list and leave the cursor where
    // the person here put it.
    let (mut state, events) = state_with_event_queue();
    state.create_pane_now().unwrap();
    let mine = state.panes[0].id;

    events.borrow_mut().push(BackendEvent::Created {
        pane: 99,
        rows: 24,
        cols: 80,
        requested: false,
        title: None,
    });
    state.poll();

    assert_eq!(state.panes.len(), 2);
    assert_eq!(state.panes[1].id, 99);
    assert_eq!(
        state.active_pane_id(),
        Some(mine),
        "the active pane must not move"
    );
}

#[test]
fn a_pane_reported_twice_is_only_taken_once() {
    // A client can be told about a pane it already has — reconnecting to a
    // session replays what is open. Adopting it again would duplicate the
    // tab and orphan the first emulator.
    let (mut state, events) = state_with_event_queue();
    for _ in 0..2 {
        events.borrow_mut().push(BackendEvent::Created {
            pane: 7,
            rows: 24,
            cols: 80,
            requested: false,
            title: None,
        });
        state.poll();
    }

    assert_eq!(state.panes.len(), 1);
}

#[test]
fn a_title_waits_for_the_pane_it_was_asked_for() {
    // The label is chosen when the pane is requested and applied when it
    // arrives, so a startup command keeps its name across the round trip.
    let (mut state, _events) = state_with_event_queue();

    state
        .create_pane_with_now(Some("cargo test"), Some("tests"))
        .unwrap();

    assert_eq!(state.panes[0].title, "tests");
}

#[test]
fn a_pane_from_elsewhere_does_not_take_a_title_this_client_is_waiting_on() {
    // The queue belongs to what this client asked for. Handing its label to
    // someone else's pane would put the wrong name on both.
    let (mut state, events) = state_with_event_queue();
    state
        .create_pane_with(Some("cargo test"), Some("tests"))
        .unwrap();

    events.borrow_mut().push(BackendEvent::Created {
        pane: 99,
        rows: 24,
        cols: 80,
        requested: false,
        title: None,
    });
    state.poll();
    // Now the requested one arrives and claims the label it was given.
    state.poll();

    let theirs = state.panes.iter().find(|p| p.id == 99).expect("their pane");
    assert_ne!(theirs.title, "tests");
    let mine = state.panes.iter().find(|p| p.id != 99).expect("my pane");
    assert_eq!(mine.title, "tests");
}
