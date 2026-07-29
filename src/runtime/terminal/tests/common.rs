use crate::backend::{BackendEvent, PaneId};
use crate::runtime::terminal::TerminalState;

/// A state with a FakeBackend plus the shared handle for injecting
/// synthetic backend events into the next `poll` call.
pub(super) fn state_with_event_queue() -> (
    TerminalState,
    std::rc::Rc<std::cell::RefCell<Vec<BackendEvent>>>,
) {
    let backend = crate::test_util::FakeBackend::default();
    let events = backend.pending_events.clone();
    (TerminalState::new(Some(Box::new(backend)), false), events)
}

/// A single 10x40 pane whose program has already emitted `modes`, with
/// the payloads recorded during setup discarded so a test sees only what
/// the scroll itself wrote. The pane's centre is therefore column 21,
/// row 6.
pub(super) fn state_with_pane_in_modes(modes: &[u8]) -> (TerminalState, PaneId) {
    let (mut state, events) = state_with_event_queue();
    state.create_pane_now().unwrap();
    let id = state.panes[0].id;
    state.resize_visible_panes(&[(id, 10, 40)]);
    events.borrow_mut().push(BackendEvent::Output {
        pane: id,
        data: modes.to_vec(),
    });
    state.poll();
    if let Some(backend) = &mut state.backend {
        backend.send_input(id, b"").ok();
    }
    (state, id)
}

/// Plain shell output taller than the 10-row test pane, so lines actually
/// scroll off the top and land in the emulator's scrollback. Without
/// overflow there is no history and nothing to scroll into.
pub(super) fn shell_output_past_one_screen() -> Vec<u8> {
    (0..20).fold(Vec::new(), |mut out, i| {
        out.extend_from_slice(format!("line{i}\r\n").as_bytes());
        out
    })
}

/// Payloads written to the PTY after `state_with_pane_in_modes` set up
/// the pane, i.e. everything past its trailing empty marker payload.
pub(super) fn payloads_after_setup(state: &TerminalState) -> Vec<Vec<u8>> {
    let sent = state.fake_backend_sent().unwrap();
    let marker = sent.iter().rposition(|p| p.is_empty()).unwrap();
    sent[marker + 1..].to_vec()
}

pub(super) fn state_with_fake() -> TerminalState {
    let backend = Box::new(crate::test_util::FakeBackend::default());
    TerminalState::new(Some(backend), false)
}
