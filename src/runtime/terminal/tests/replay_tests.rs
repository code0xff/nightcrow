use super::common::state_with_event_queue;
use crate::backend::BackendEvent;
use std::time::Instant;

#[test]
fn replay_queries_are_suppressed_until_live_output_and_user_input_still_passes() {
    let (mut state, events) = state_with_event_queue();
    state.create_pane_now().unwrap();
    let pane = state.panes[0].id;

    events.borrow_mut().push(BackendEvent::ReplayOutput {
        pane,
        data: b"\x1b[6n".to_vec(),
    });
    state.poll();
    assert!(state.fake_backend_sent().unwrap().is_empty());

    state.send_input(b"user input");
    assert_eq!(
        state.fake_backend_sent().unwrap(),
        vec![b"user input".to_vec()]
    );

    events.borrow_mut().push(BackendEvent::ReplayComplete);
    state.poll();
    assert_eq!(
        state.fake_backend_sent().unwrap(),
        vec![b"user input".to_vec()]
    );

    events.borrow_mut().push(BackendEvent::Output {
        pane,
        data: b"\x1b[6n".to_vec(),
    });
    state.poll();
    assert_eq!(
        state.fake_backend_sent().unwrap(),
        vec![b"user input".to_vec(), b"\x1b[1;1R".to_vec()]
    );
}

#[test]
fn replay_completion_flushes_synchronized_screen_and_title_without_replying() {
    let (mut state, events) = state_with_event_queue();
    state.create_pane_now().unwrap();
    let pane = state.panes[0].id;

    events.borrow_mut().push(BackendEvent::ReplayOutput {
        pane,
        data: b"\x1b[?2026h\x1b[1;1Hhistory\x1b]2;Replay title\x07\x1b[6n".to_vec(),
    });
    state.poll();
    assert!(state.fake_backend_sent().unwrap().is_empty());
    assert_eq!(state.panes[0].title, "shell 1");

    events.borrow_mut().push(BackendEvent::ReplayComplete);
    let (_, changed) = state.poll_at_with_activity(Instant::now());

    assert!(changed);
    assert_eq!(state.panes[0].title, "Replay title");
    assert!(state.emulators[&pane].screen_current());
    assert!(state.fake_backend_sent().unwrap().is_empty());

    events.borrow_mut().push(BackendEvent::Output {
        pane,
        data: b"\x1b[6n".to_vec(),
    });
    state.poll();
    assert_eq!(
        state.fake_backend_sent().unwrap(),
        vec![b"\x1b[1;8R".to_vec()]
    );
}
