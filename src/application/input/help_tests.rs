use super::*;
use crate::application::input::dispatch::dispatch_key;
use crossterm::event::{KeyEventKind, KeyEventState};

fn key(code: KeyCode) -> KeyEvent {
    KeyEvent {
        code,
        modifiers: KeyModifiers::NONE,
        kind: KeyEventKind::Press,
        state: KeyEventState::NONE,
    }
}

fn workspace_with_help_open() -> Workspace {
    let mut ws = Workspace::new(key(KeyCode::Char('f')));
    ws.help.toggle();
    ws
}

#[test]
fn esc_closes_the_overlay() {
    let mut ws = workspace_with_help_open();

    dispatch_key(&mut ws, key(KeyCode::Esc));

    assert!(!ws.help.open);
}

#[test]
fn either_key_that_opens_the_overlay_also_closes_it() {
    for code in [KeyCode::Char('?'), KeyCode::Char('h')] {
        let mut ws = workspace_with_help_open();

        dispatch_key(&mut ws, key(code));

        assert!(!ws.help.open, "{code:?} left the overlay up");
    }
}

#[test]
fn j_and_k_move_the_view_and_k_at_the_top_stays_there() {
    let mut ws = workspace_with_help_open();

    dispatch_key(&mut ws, key(KeyCode::Char('j')));
    assert_eq!(ws.help.scroll_offset(), 1);

    dispatch_key(&mut ws, key(KeyCode::Char('k')));
    dispatch_key(&mut ws, key(KeyCode::Char('k')));
    assert_eq!(ws.help.scroll_offset(), 0, "scrolled past the first line");
}

#[test]
fn an_unbound_key_is_swallowed_rather_than_reaching_what_is_underneath() {
    let mut ws = workspace_with_help_open();

    let outcome = dispatch_key(&mut ws, key(KeyCode::Char('t')));

    assert_eq!(outcome, KeyOutcome::Continue);
    assert!(ws.help.open, "the overlay closed on an unbound key");
}

#[test]
fn ctrl_c_closes_the_overlay_like_it_cancels_every_other_mode() {
    let mut ws = workspace_with_help_open();

    dispatch_key(
        &mut ws,
        KeyEvent {
            modifiers: KeyModifiers::CONTROL,
            ..key(KeyCode::Char('c'))
        },
    );

    assert!(!ws.help.open);
}
