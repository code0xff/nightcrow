//! Keys for the open-repo dialog. It owns every key while up, so all bindings
//! here are bare.

use crate::application::input::dispatch::{KeyOutcome, ProjectRequest, text_input_char};
use crate::workspace::Workspace;
use crossterm::event::{KeyCode, KeyEvent};

pub(crate) fn handle_repo_input_key(ws: &mut Workspace, key: KeyEvent) -> KeyOutcome {
    // The browser takes the keys while open; the field's text cannot change
    // until it hands a path back.
    if ws.repo_input.picker.is_some() {
        return handle_picker_key(ws, key);
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
        // Either vertical key means "the list" in a single-line field.
        KeyCode::Down | KeyCode::Up => ws.repo_input_browse(),
        // Tab only completes; the browser opens with ↓ alone. `BackTab` is
        // unhandled because completion never cycles.
        KeyCode::Tab => ws.repo_input_complete(),
        _ => {
            if let Some(c) = text_input_char(key) {
                ws.repo_input_push(c);
            }
        }
    }
    KeyOutcome::Continue
}

/// Enter on a row opens it: selecting into the field and confirming there was
/// two keys for one gesture. `→` still expands, so descending without opening
/// stays possible.
fn handle_picker_key(ws: &mut Workspace, key: KeyEvent) -> KeyOutcome {
    match key.code {
        // One Esc leaves the browser with the field's text intact, a second
        // cancels the dialog.
        KeyCode::Esc => {
            ws.repo_input_close_browser();
            KeyOutcome::Continue
        }
        KeyCode::Enter => {
            ws.repo_input_pick();
            if let crate::workspace::RepoInputResult::Open(path) = ws.confirm_repo_input() {
                return KeyOutcome::Project(ProjectRequest::Open(path));
            }
            KeyOutcome::Continue
        }
        KeyCode::Down | KeyCode::Char('j') => {
            ws.repo_picker_move(true);
            KeyOutcome::Continue
        }
        KeyCode::Up | KeyCode::Char('k') => {
            ws.repo_picker_move(false);
            KeyOutcome::Continue
        }
        KeyCode::Right => {
            ws.repo_picker_expand();
            KeyOutcome::Continue
        }
        KeyCode::Left => {
            ws.repo_picker_collapse();
            KeyOutcome::Continue
        }
        _ => KeyOutcome::Continue,
    }
}
