use super::common::state_with_event_queue;
use crate::backend::BackendEvent;
use std::time::Instant;

#[test]
fn terminal_poll_activity_is_false_when_no_backend_event_arrives() {
    let (mut state, _events) = state_with_event_queue();

    let (_, changed) = state.poll_at_with_activity(Instant::now());

    assert!(!changed, "an idle terminal must not request a frame");
}

#[test]
fn terminal_poll_activity_reports_output_and_resize_events() {
    let (mut state, events) = state_with_event_queue();
    state.create_pane_now().unwrap();
    let pane = state.panes[0].id;

    events.borrow_mut().push(BackendEvent::Output {
        pane,
        data: b"output".to_vec(),
    });
    let (_, output_changed) = state.poll_at_with_activity(Instant::now());
    assert!(output_changed, "PTY output must request a frame");

    events.borrow_mut().push(BackendEvent::Resized {
        pane,
        rows: 24,
        cols: 80,
    });
    let (_, resize_changed) = state.poll_at_with_activity(Instant::now());
    assert!(resize_changed, "a confirmed resize must request a frame");
}
