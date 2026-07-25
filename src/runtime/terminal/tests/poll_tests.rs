use super::common::*;
use super::*;
use crate::backend::BackendEvent;

#[test]
fn poll_applies_osc_title_to_pane() {
    let (mut state, events) = state_with_event_queue();
    state.create_pane().unwrap();
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
    state.create_pane_with(None, Some("shell")).unwrap();
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
    state.create_pane().unwrap();
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
    state.create_pane().unwrap();
    let id = state.panes[0].id;

    events.borrow_mut().push(BackendEvent::Output {
        pane: id,
        data: b"\x1b]2;first\x07\x1b]2;second\x07".to_vec(),
    });
    state.poll();

    assert_eq!(state.panes[0].title, "second");
}
