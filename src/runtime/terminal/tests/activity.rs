use super::common::state_with_event_queue;
use crate::backend::{BackendEvent, PaneId};
use crate::runtime::terminal::{MAX_VISIBLE_NORMAL, TerminalState};
use std::cell::RefCell;
use std::rc::Rc;
use std::time::Instant;

type EventQueue = Rc<RefCell<Vec<BackendEvent>>>;

/// A state with one more pane than the split window shows, focused on the
/// first so the last one is outside it.
fn state_with_hidden_pane() -> (TerminalState, EventQueue, PaneId, PaneId) {
    let (mut state, events) = state_with_event_queue();
    for _ in 0..=MAX_VISIBLE_NORMAL {
        state.create_pane_now().unwrap();
    }
    state.active = 0;
    state.sync_visible_window();
    let visible = state.panes[0].id;
    let hidden = state.panes[MAX_VISIBLE_NORMAL].id;
    assert!(
        !state.pane_is_visible(hidden),
        "the extra pane is outside the window"
    );
    (state, events, visible, hidden)
}

fn poll_frame(state: &mut TerminalState, events: &EventQueue, event: BackendEvent) -> bool {
    events.borrow_mut().push(event);
    state.poll_at_with_activity(Instant::now()).frame_changed
}

fn pane_text(state: &TerminalState, pane: PaneId) -> String {
    let mut text = String::new();
    state
        .screen_for_pane(pane)
        .unwrap()
        .cell(0, 0)
        .unwrap()
        .append_contents(&mut text);
    text
}

#[test]
fn terminal_poll_activity_is_false_when_no_backend_event_arrives() {
    let (mut state, _events) = state_with_event_queue();

    let outcome = state.poll_at_with_activity(Instant::now());

    assert!(
        !outcome.frame_changed,
        "an idle terminal must not request a frame"
    );
}

#[test]
fn terminal_poll_activity_reports_output_and_resize_events() {
    let (mut state, events) = state_with_event_queue();
    state.create_pane_now().unwrap();
    let pane = state.panes[0].id;

    let output_changed = poll_frame(
        &mut state,
        &events,
        BackendEvent::Output {
            pane,
            data: b"output".to_vec(),
        },
    );
    assert!(output_changed, "PTY output must request a frame");

    let resize_changed = poll_frame(
        &mut state,
        &events,
        BackendEvent::Resized {
            pane,
            rows: 24,
            cols: 80,
        },
    );
    assert!(resize_changed, "a confirmed resize must request a frame");
}

#[test]
fn hidden_pane_output_reaches_the_emulator_without_requesting_a_frame() {
    let (mut state, events, visible, hidden) = state_with_hidden_pane();

    let hidden_changed = poll_frame(
        &mut state,
        &events,
        BackendEvent::Output {
            pane: hidden,
            data: b"hidden".to_vec(),
        },
    );

    assert!(!hidden_changed, "hidden-pane bytes cannot change the frame");
    assert_eq!(
        pane_text(&state, hidden),
        "h",
        "hidden bytes are still applied"
    );

    let visible_changed = poll_frame(
        &mut state,
        &events,
        BackendEvent::Output {
            pane: visible,
            data: b"shown".to_vec(),
        },
    );
    assert!(visible_changed, "visible-pane output must request a frame");
}

#[test]
fn hidden_pane_replay_output_and_resize_do_not_request_a_frame() {
    let (mut state, events, visible, hidden) = state_with_hidden_pane();

    let replay_changed = poll_frame(
        &mut state,
        &events,
        BackendEvent::ReplayOutput {
            pane: hidden,
            data: b"history".to_vec(),
        },
    );
    assert!(!replay_changed, "hidden replay cannot change the frame");
    assert_eq!(pane_text(&state, hidden), "h");

    let hidden_resize = poll_frame(
        &mut state,
        &events,
        BackendEvent::Resized {
            pane: hidden,
            rows: 10,
            cols: 20,
        },
    );
    assert!(
        !hidden_resize,
        "resizing a hidden pane cannot change the frame"
    );

    let visible_resize = poll_frame(
        &mut state,
        &events,
        BackendEvent::Resized {
            pane: visible,
            rows: 10,
            cols: 20,
        },
    );
    assert!(
        visible_resize,
        "resizing a visible pane must request a frame"
    );

    let repeated = poll_frame(
        &mut state,
        &events,
        BackendEvent::Resized {
            pane: visible,
            rows: 10,
            cols: 20,
        },
    );
    assert!(!repeated, "a repeated size confirmation is a no-op");
}

#[test]
fn hidden_pane_title_change_requests_a_frame() {
    let (mut state, events, _visible, hidden) = state_with_hidden_pane();

    let changed = poll_frame(
        &mut state,
        &events,
        BackendEvent::Output {
            pane: hidden,
            data: b"\x1b]2;hidden title\x07".to_vec(),
        },
    );

    assert!(changed, "a pane title is visible even outside the window");
    assert_eq!(state.panes[MAX_VISIBLE_NORMAL].title, "hidden title");
}

#[test]
fn hidden_pane_attention_and_exit_request_a_frame() {
    let (mut state, events, _visible, hidden) = state_with_hidden_pane();

    let attention = poll_frame(
        &mut state,
        &events,
        BackendEvent::Attention { pane: hidden },
    );
    assert!(attention, "a new unread badge must request a frame");
    assert!(state.has_unread_attention());
    state.acknowledge_attention();

    let exited = poll_frame(&mut state, &events, BackendEvent::Exited { pane: hidden });
    assert!(exited, "closing a pane changes the pane tab row");
    assert_eq!(state.panes.len(), MAX_VISIBLE_NORMAL);
}
