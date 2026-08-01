//! Keys for the open-repo dialog: the path field, and the directory browser it
//! can open. The dialog owns every key while it is up, so these are all bare
//! keys — no leader, and no chord to keep clear of the app's own bindings.

use crate::application::input::dispatch::{KeyOutcome, ProjectRequest, text_input_char};
use crate::workspace::Workspace;
use crossterm::event::{KeyCode, KeyEvent};

pub(crate) fn handle_repo_input_key(ws: &mut Workspace, key: KeyEvent) -> KeyOutcome {
    // The browser takes the keys while it is open; the field is still on screen
    // below it, but its text cannot change until the browser hands a path back.
    if ws.repo_input.picker.is_some() {
        handle_picker_key(ws, key);
        return KeyOutcome::Continue;
    }
    match key.code {
        KeyCode::Esc => ws.cancel_repo_input(),
        KeyCode::Enter => {
            if let crate::workspace::RepoInputResult::Open(path) = ws.confirm_repo_input() {
                return KeyOutcome::Project(ProjectRequest::Open(path));
            }
        }
        KeyCode::Backspace => {
            if ws.repo_input.buf.is_empty() {
                ws.cancel_repo_input();
            } else {
                ws.repo_input_pop();
            }
        }
        // The caret is always at the end of the buffer, so these can't move
        // it; they mean "keep this path and let me extend it".
        KeyCode::Right | KeyCode::End => ws.repo_input_accept_prefill(),
        // Down opens the browser, matching where every autocomplete puts its
        // list. Up too: reaching for either vertical key means "the list",
        // and neither can mean anything else in a single-line field.
        KeyCode::Down | KeyCode::Up => ws.repo_input_browse(),
        // `BackTab` is deliberately unhandled: completion here never cycles, so
        // there is nothing for a reverse Tab to step back through.
        // Tab only completes the path — the browser opens with ↓ alone, so
        // repeated Tab presses never leave the field.
        KeyCode::Tab => ws.repo_input_complete(),
        _ => {
            if let Some(c) = text_input_char(key) {
                ws.repo_input_push(c);
            }
        }
    }
    KeyOutcome::Continue
}

/// `Enter` selects here rather than opening — the browser fills the field, and
/// the field's Enter remains the single place a repo is opened. That splits the
/// meaning of Enter between the two surfaces, which is why `→` alone expands
/// (unlike the in-repo tree view, where Enter expands too).
fn handle_picker_key(ws: &mut Workspace, key: KeyEvent) {
    match key.code {
        // One Esc leaves the browser, a second cancels the dialog: the field's
        // text survives the first, so a browse can be abandoned without
        // retyping the path it started from.
        KeyCode::Esc => ws.repo_input_close_browser(),
        KeyCode::Enter => ws.repo_input_pick(),
        KeyCode::Down | KeyCode::Char('j') => ws.repo_picker_move(true),
        KeyCode::Up | KeyCode::Char('k') => ws.repo_picker_move(false),
        KeyCode::Right => ws.repo_picker_expand(),
        KeyCode::Left => ws.repo_picker_collapse(),
        _ => {}
    }
}
