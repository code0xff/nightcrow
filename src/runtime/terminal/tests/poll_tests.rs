use super::common::*;
use super::*;
use crate::backend::BackendEvent;
use std::time::{Duration, Instant};

#[test]
fn poll_applies_osc_title_to_pane() {
    let (mut state, events) = state_with_event_queue();
    state.create_pane_now().unwrap();
    let id = state.panes[0].id;

    events.borrow_mut().push(BackendEvent::Output {
        pane: id,
        data: b"\x1b]2;claude\x07".to_vec(),
    });
    state.poll();

    assert_eq!(state.panes[0].title, "claude");
}

#[test]
fn poll_keeps_title_when_output_sets_none() {
    let (mut state, events) = state_with_event_queue();
    state.create_pane_with_now(None, Some("shell")).unwrap();
    let id = state.panes[0].id;

    events.borrow_mut().push(BackendEvent::Output {
        pane: id,
        data: b"plain output\x1b]2;\x07".to_vec(),
    });
    state.poll();

    // Plain output (and an empty OSC title) must not clobber the label.
    assert_eq!(state.panes[0].title, "shell");
}

#[test]
fn poll_forwards_terminal_query_reply_to_pty() {
    let (mut state, events) = state_with_event_queue();
    state.create_pane_now().unwrap();
    let id = state.panes[0].id;

    // DSR 6 — the program asks for the cursor position; the emulator's
    // reply must reach the backend PTY.
    events.borrow_mut().push(BackendEvent::Output {
        pane: id,
        data: b"\x1b[6n".to_vec(),
    });
    state.poll();

    let sent = state.fake_backend_sent().unwrap();
    assert_eq!(sent, vec![b"\x1b[1;1R".to_vec()]);
}

#[test]
fn consume_csi_skips_del_byte_per_ecma48() {
    // ESC [ 3 1 DEL m sgr — the DEL must be ignored without terminating
    // the sequence early. The trailing 'm' is the real final byte; the
    // following "ok" should survive intact.
    let out = strip_escape_sequences(b"\x1b[31\x7fmok");
    assert_eq!(out, "ok");
}

#[test]
fn strip_escape_sequences_preserves_newline_after_malformed_csi() {
    // A CSI body interrupted by a control byte must leave that byte for
    // the outer pass so prompt-buffer flush on `\n` still fires.
    let out = strip_escape_sequences(b"\x1b[31\ndone\n");
    assert_eq!(out, "\ndone\n");
}

#[test]
fn later_title_replaces_earlier_within_one_poll() {
    let (mut state, events) = state_with_event_queue();
    state.create_pane_now().unwrap();
    let id = state.panes[0].id;

    events.borrow_mut().push(BackendEvent::Output {
        pane: id,
        data: b"\x1b]2;first\x07\x1b]2;second\x07".to_vec(),
    });
    state.poll();

    assert_eq!(state.panes[0].title, "second");
}

fn title_event(pane: crate::backend::PaneId, title: &str) -> BackendEvent {
    BackendEvent::Output {
        pane,
        data: format!("\x1b]2;{title}\x07").into_bytes(),
    }
}

#[test]
fn an_animated_title_settling_marks_attention() {
    let (mut state, events) = state_with_event_queue();
    state.create_pane_now().unwrap();
    let pane = state.panes[0].id;
    let start = Instant::now();

    for (offset, title) in [(0, "⠋ repo"), (300, "⠙ repo"), (600, "⠹ repo")] {
        events.borrow_mut().push(title_event(pane, title));
        state.poll_at(start + Duration::from_millis(offset));
    }
    assert!(!state.has_unread_attention(), "animation is still active");

    state.poll_at(start + Duration::from_millis(1_400));

    assert!(state.has_unread_attention());
}

#[test]
fn acknowledging_an_animation_prevents_a_stale_attention_event() {
    let (mut state, events) = state_with_event_queue();
    state.create_pane_now().unwrap();
    let pane = state.panes[0].id;
    let start = Instant::now();

    for (offset, title) in [(0, "⠋ repo"), (300, "⠙ repo"), (600, "⠹ repo")] {
        events.borrow_mut().push(title_event(pane, title));
        state.poll_at(start + Duration::from_millis(offset));
    }
    state.acknowledge_attention();
    state.poll_at(start + Duration::from_millis(1_400));

    assert!(!state.has_unread_attention());
}

#[test]
fn ordinary_sparse_title_changes_do_not_mark_attention() {
    let (mut state, events) = state_with_event_queue();
    state.create_pane_now().unwrap();
    let pane = state.panes[0].id;
    let start = Instant::now();

    events.borrow_mut().push(title_event(pane, "editor"));
    state.poll_at(start);
    events.borrow_mut().push(title_event(pane, "shell"));
    state.poll_at(start + Duration::from_millis(300));
    state.poll_at(start + Duration::from_millis(1_100));

    assert!(!state.has_unread_attention());
}

#[test]
fn a_terminal_bell_marks_attention_until_acknowledged() {
    let (mut state, events) = state_with_event_queue();
    state.create_pane_now().unwrap();
    let pane = state.panes[0].id;

    events.borrow_mut().push(BackendEvent::Output {
        pane,
        data: b"\x07".to_vec(),
    });
    state.poll();

    assert!(state.has_unread_attention());
    state.acknowledge_attention();
    assert!(!state.has_unread_attention());
}

#[test]
fn a_plugin_attention_event_marks_the_pane_this_client_holds() {
    let (mut state, events) = state_with_event_queue();
    state.create_pane_now().unwrap();
    let pane = state.panes[0].id;

    events.borrow_mut().push(BackendEvent::Attention { pane });
    state.poll();

    assert!(state.has_unread_attention());
}

#[test]
fn a_plugin_attention_event_for_an_unknown_pane_is_ignored() {
    let (mut state, events) = state_with_event_queue();
    state.create_pane_now().unwrap();
    let absent = state.panes[0].id + 1;

    events
        .borrow_mut()
        .push(BackendEvent::Attention { pane: absent });
    state.poll();

    assert!(
        !state.has_unread_attention(),
        "a marker for a pane this client has nothing to show for"
    );
}

#[test]
fn a_pane_exit_marks_attention_after_removing_the_pane() {
    let (mut state, events) = state_with_event_queue();
    state.create_pane_now().unwrap();
    let pane = state.panes[0].id;

    events.borrow_mut().push(BackendEvent::Exited { pane });
    state.poll();

    assert!(state.panes.is_empty());
    assert!(state.has_unread_attention());
}

/// A pane the backend reports without this client having asked for it — what
/// happens when another client on a shared session opens one.
mod panes_from_elsewhere {
    use super::super::common::*;
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
}
