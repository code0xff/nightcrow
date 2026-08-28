//! Following a pane size this client does not decide.
//!
//! In a shared session one client's layout sets the PTY sizes and the rest
//! render the grid they are given. These are the client's half of that.

use super::common::state_with_event_queue;
use crate::backend::{BackendEvent, ResizeOutcome};
use crate::runtime::terminal::TerminalState;
use std::time::{Duration, Instant};

type EventQueue = std::rc::Rc<std::cell::RefCell<Vec<BackendEvent>>>;
type ResizeCalls = std::rc::Rc<std::cell::RefCell<Vec<(u32, u16, u16)>>>;

fn state_with_pending_resize() -> (TerminalState, EventQueue, ResizeCalls) {
    let backend = crate::test_util::FakeBackend::with_resize_outcome(ResizeOutcome::Pending);
    let events = backend.pending_events.clone();
    let resized = backend.resized.clone();
    let state = TerminalState::new(Some(Box::new(backend)), false);
    (state, events, resized)
}

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
fn the_owner_keeps_its_desired_size_separate_from_the_confirmed_size() {
    // The emulator follows what the PTY reports, while the desired layout stays
    // intact so an older acknowledgement cannot overwrite the final width.
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

#[test]
fn an_unconfirmed_resize_is_retried_after_the_deadline() {
    let (mut state, _events, resized) = state_with_pending_resize();
    state.create_pane_now().unwrap();
    let pane = state.panes[0].id;
    let start = Instant::now();

    state.resize_visible_panes_at(&[(pane, 30, 100)], start);
    state.resize_visible_panes_at(&[(pane, 30, 100)], start + Duration::from_millis(99));
    assert_eq!(resized.borrow().len(), 1, "pending resize is not flooded");

    state.resize_visible_panes_at(&[(pane, 30, 100)], start + Duration::from_millis(100));
    assert_eq!(resized.borrow().len(), 2, "an unanswered resize retries");
}

#[test]
fn a_late_ack_cannot_strand_the_emulator_at_an_old_width() {
    let (mut state, events, resized) = state_with_pending_resize();
    state.create_pane_now().unwrap();
    let pane = state.panes[0].id;
    let start = Instant::now();

    state.resize_visible_panes_at(&[(pane, 30, 100)], start);
    state.resize_visible_panes_at(&[(pane, 40, 120)], start + Duration::from_millis(1));
    events.borrow_mut().push(BackendEvent::Resized {
        pane,
        rows: 30,
        cols: 100,
    });
    state.poll_at(start + Duration::from_millis(2));
    assert_eq!(state.screen_for_pane(pane).unwrap().size(), (30, 100));

    state.resize_visible_panes_at(&[(pane, 40, 120)], start + Duration::from_millis(3));
    assert_eq!(
        resized.borrow().last().copied(),
        Some((pane, 40, 120)),
        "desired and confirmed differ, so the latest width is requested again"
    );
    assert_eq!(resized.borrow().len(), 3);

    events.borrow_mut().push(BackendEvent::Resized {
        pane,
        rows: 40,
        cols: 120,
    });
    state.poll_at(start + Duration::from_millis(4));
    state.resize_visible_panes_at(&[(pane, 40, 120)], start + Duration::from_secs(1));
    assert_eq!(state.screen_for_pane(pane).unwrap().size(), (40, 120));
    assert_eq!(resized.borrow().len(), 3, "confirmed size stays settled");
}

#[test]
fn a_failed_resize_is_not_recorded_as_applied() {
    let mut backend = crate::test_util::FakeBackend::default();
    backend.resize_error = true;
    let resized = backend.resized.clone();
    let mut state = TerminalState::new(Some(Box::new(backend)), false);
    state.create_pane_now().unwrap();
    let pane = state.panes[0].id;
    let original = state.screen_for_pane(pane).unwrap().size();
    let start = Instant::now();

    state.resize_visible_panes_at(&[(pane, 30, 100)], start);

    assert_eq!(state.screen_for_pane(pane).unwrap().size(), original);
    assert_ne!(state.confirmed_content_size.get(&pane), Some(&(30, 100)));
    assert!(state.pending_content_size.contains_key(&pane));
    state.resize_visible_panes_at(&[(pane, 30, 100)], start + Duration::from_millis(100));
    assert_eq!(
        resized.borrow().len(),
        2,
        "a failed resize remains retryable"
    );
}

#[test]
fn an_ack_for_a_removed_pane_does_not_recreate_its_size_state() {
    let (mut state, events) = state_with_event_queue();
    state.create_pane_now().unwrap();
    let pane = state.panes[0].id;
    state.remove_pane_state(pane);
    state.panes.clear();
    events.borrow_mut().push(BackendEvent::Resized {
        pane,
        rows: 30,
        cols: 100,
    });

    state.poll();

    assert!(!state.confirmed_content_size.contains_key(&pane));
    assert!(!state.pending_content_size.contains_key(&pane));
}
