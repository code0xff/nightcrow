//! Following a pane size this client does not decide.
//!
//! In a shared session one client's layout sets the PTY sizes and the rest
//! render the grid they are given. These are the client's half of that.

use super::common::state_with_event_queue;
use crate::backend::BackendEvent;

#[test]
fn a_client_owns_its_sizes_until_a_session_says_otherwise() {
    // A local backend's PTYs are nobody else's, so nothing has to tell it.
    let (state, _events) = state_with_event_queue();

    assert!(state.owns_size);
}

#[test]
fn a_client_that_does_not_own_the_sizing_leaves_the_panes_alone() {
    // Its layout is not what the child was told. Sending its own size would
    // take the PTY away from the client that does own it, one frame at a time.
    let (mut state, events) = state_with_event_queue();
    state.create_pane_now().unwrap();
    let pane = state.panes[0].id;
    state.resize_visible_panes(&[(pane, 24, 80)]);
    events
        .borrow_mut()
        .push(BackendEvent::SizeOwnership { owned: false });
    state.poll();

    state.resize_visible_panes(&[(pane, 40, 120)]);

    assert_eq!(
        state.last_content_size.get(&pane),
        Some(&(24, 80)),
        "the pane is still at the size the session set"
    );
    // The layout is still recorded as what a new pane would be born at: it is
    // this client's best guess, and the owner corrects it on the next frame.
    assert_eq!(state.size, (40, 120));
}

#[test]
fn a_spectator_follows_the_size_the_session_reports() {
    // The emulator has to wrap where the child does, whatever this client's
    // own area happens to be.
    let (mut state, events) = state_with_event_queue();
    state.create_pane_now().unwrap();
    let pane = state.panes[0].id;
    state.resize_visible_panes(&[(pane, 24, 80)]);
    events.borrow_mut().extend([
        BackendEvent::SizeOwnership { owned: false },
        BackendEvent::Resized {
            pane,
            rows: 30,
            cols: 100,
        },
    ]);

    state.poll();

    assert_eq!(state.last_content_size.get(&pane), Some(&(30, 100)));
    let screen = state.emulators.get(&pane).expect("an emulator").view();
    assert_eq!(screen.size(), (30, 100));
}

#[test]
fn the_owner_follows_a_size_it_did_not_ask_for_without_asking_again() {
    // Its request can come back clamped. The emulator has to follow the PTY,
    // but the record of what was *asked for* must not, or every frame would
    // re-send a size the hub will clamp the same way — forever.
    let (mut state, events) = state_with_event_queue();
    state.create_pane_now().unwrap();
    let pane = state.panes[0].id;
    state.resize_visible_panes(&[(pane, 24, 80)]);
    events.borrow_mut().push(BackendEvent::Resized {
        pane,
        rows: 24,
        cols: 60,
    });

    state.poll();

    assert_eq!(
        state.last_content_size.get(&pane),
        Some(&(24, 80)),
        "the owner keeps what it asked for"
    );
    let screen = state.emulators.get(&pane).expect("an emulator").view();
    assert_eq!(screen.size(), (24, 60), "but wraps where the child does");
}

#[test]
fn taking_the_sizing_back_re_applies_this_client_layout() {
    // The panes are at someone else's sizes, and the layout has not changed —
    // so without forgetting what was applied, the skip-if-unchanged check would
    // leave them there.
    let (mut state, events) = state_with_event_queue();
    state.create_pane_now().unwrap();
    let pane = state.panes[0].id;
    state.resize_visible_panes(&[(pane, 24, 80)]);
    events
        .borrow_mut()
        .push(BackendEvent::SizeOwnership { owned: false });
    state.poll();
    events
        .borrow_mut()
        .push(BackendEvent::SizeOwnership { owned: true });

    state.poll();

    assert!(
        state.last_content_size.is_empty(),
        "what was applied is forgotten, so the next frame fits every pane"
    );
    state.resize_visible_panes(&[(pane, 24, 80)]);
    assert_eq!(state.last_content_size.get(&pane), Some(&(24, 80)));
}
