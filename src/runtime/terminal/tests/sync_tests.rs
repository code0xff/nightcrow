use super::common::*;
use super::*;
use crate::backend::BackendEvent;
use std::time::{Duration, Instant};

/// Past vte's window for an open synchronized update, so the sweep sees an
/// expired one without the test having to sleep through it.
const PAST_SYNC_WINDOW: Duration = Duration::from_millis(200);

fn first_line(state: &TerminalState, id: crate::backend::PaneId) -> String {
    let screen = state.screen_for_pane(id).unwrap();
    let (_, cols) = screen.size();
    let mut out = String::new();
    for col in 0..cols {
        screen.cell(0, col).unwrap().append_contents(&mut out);
    }
    out.trim_end().to_string()
}

#[test]
fn a_pane_left_inside_a_synchronized_update_repaints_on_the_clock() {
    // A program killed between BSU and ESU — on exit, or re-execing to update
    // itself — leaves the update open, and the shell that takes the pane back
    // never writes enough to reach the processor's buffer cap. Without the
    // clock the pane would never paint again.
    let (mut state, events) = state_with_event_queue();
    state.create_pane_now().unwrap();
    let id = state.panes[0].id;

    events.borrow_mut().push(BackendEvent::Output {
        pane: id,
        data: b"\x1b[?2026h\x1b[1;1Hprompt$ ".to_vec(),
    });
    state.poll();
    assert_eq!(first_line(&state, id), "");

    state.settle_sync_updates(Instant::now() + PAST_SYNC_WINDOW);

    assert_eq!(first_line(&state, id), "prompt$");
}

#[test]
fn a_synchronized_update_inside_its_window_is_left_alone() {
    let (mut state, events) = state_with_event_queue();
    state.create_pane_now().unwrap();
    let id = state.panes[0].id;

    events.borrow_mut().push(BackendEvent::Output {
        pane: id,
        data: b"\x1b[?2026h\x1b[1;1Hhalf a frame".to_vec(),
    });
    // The poll's own sweep runs on the real clock: a live update survives it.
    state.poll();

    assert_eq!(first_line(&state, id), "");
}

#[test]
fn a_reply_held_back_by_a_settled_update_reaches_the_pty() {
    let (mut state, events) = state_with_event_queue();
    state.create_pane_now().unwrap();
    let id = state.panes[0].id;

    // DSR 6 inside the update: the program asked where the cursor is, and the
    // answer must still go out once the update is closed on the clock.
    events.borrow_mut().push(BackendEvent::Output {
        pane: id,
        data: b"\x1b[?2026h\x1b[6n".to_vec(),
    });
    state.poll();
    assert!(state.fake_backend_sent().unwrap().is_empty());

    state.settle_sync_updates(Instant::now() + PAST_SYNC_WINDOW);

    assert_eq!(
        state.fake_backend_sent().unwrap(),
        vec![b"\x1b[1;1R".to_vec()]
    );
}
