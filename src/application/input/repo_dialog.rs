//! Keys for the open-repo dialog. It owns every key while up, so all bindings
//! here are bare.

use crate::application::input::dispatch::{KeyOutcome, ProjectRequest, text_input_char};
use crate::ui::status_view::RepoInputMode;
use crate::workspace::{CloneDialogAction, Workspace};
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

pub(crate) fn handle_repo_input_key(ws: &mut Workspace, key: KeyEvent) -> KeyOutcome {
    if is_shift_tab(key) {
        let query = ws.toggle_repo_input_mode();
        return if query {
            KeyOutcome::Project(ProjectRequest::CloneStatus { job: None })
        } else {
            KeyOutcome::Continue
        };
    }
    // The browser takes the keys while open; the field's text cannot change
    // until it hands a path back.
    if ws.repo_input.picker.is_some() {
        return handle_picker_key(ws, key);
    }
    match key.code {
        KeyCode::Esc => match ws.repo_input.mode {
            RepoInputMode::CloneUrl
                if !matches!(
                    ws.repo_input.clone_status,
                    crate::ui::status_view::CloneUiStatus::Starting
                        | crate::ui::status_view::CloneUiStatus::Running
                        | crate::ui::status_view::CloneUiStatus::Discovering
                        | crate::ui::status_view::CloneUiStatus::Done(_)
                ) =>
            {
                ws.repo_input.mode = RepoInputMode::CloneParent
            }
            RepoInputMode::CloneUrl => ws.cancel_repo_input(),
            _ => ws.cancel_repo_input(),
        },
        KeyCode::Enter => match ws.repo_input.mode {
            RepoInputMode::Open => {
                if let crate::workspace::RepoInputResult::Open(path) = ws.confirm_repo_input() {
                    return KeyOutcome::Project(ProjectRequest::Open(path));
                }
            }
            RepoInputMode::CloneParent => match &ws.repo_input.clone_status {
                crate::ui::status_view::CloneUiStatus::Idle
                | crate::ui::status_view::CloneUiStatus::Discovering
                | crate::ui::status_view::CloneUiStatus::Unconfirmed
                | crate::ui::status_view::CloneUiStatus::Failed(_) => {
                    ws.confirm_clone_parent();
                }
                crate::ui::status_view::CloneUiStatus::Starting
                | crate::ui::status_view::CloneUiStatus::Running => {}
                crate::ui::status_view::CloneUiStatus::Done(_) => {
                    return clone_action(ws.submit_clone_url());
                }
            },
            RepoInputMode::CloneUrl => return clone_action(ws.submit_clone_url()),
        },
        KeyCode::Backspace => {
            if ws.repo_input.mode == RepoInputMode::Open && ws.repo_input.buf.is_empty() {
                ws.cancel_repo_input();
            } else {
                ws.repo_input_pop();
            }
        }
        // Either vertical key means "the list" in a single-line field.
        KeyCode::Down | KeyCode::Up if ws.repo_input.mode != RepoInputMode::CloneUrl => {
            ws.repo_input_browse()
        }
        // Tab only completes; BackTab switched dialog modes before this match.
        KeyCode::Tab if ws.repo_input.mode != RepoInputMode::CloneUrl => ws.repo_input_complete(),
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
            match ws.repo_input.mode {
                RepoInputMode::Open => {
                    if let crate::workspace::RepoInputResult::Open(path) = ws.confirm_repo_input() {
                        return KeyOutcome::Project(ProjectRequest::Open(path));
                    }
                }
                RepoInputMode::CloneParent => {
                    ws.confirm_clone_parent();
                }
                RepoInputMode::CloneUrl => {}
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

fn clone_action(action: CloneDialogAction) -> KeyOutcome {
    match action {
        CloneDialogAction::None => KeyOutcome::Continue,
        CloneDialogAction::Start { parent, url } => {
            KeyOutcome::Project(ProjectRequest::StartClone { parent, url })
        }
        CloneDialogAction::Open(path) => KeyOutcome::Project(ProjectRequest::Open(path)),
    }
}

fn is_shift_tab(key: KeyEvent) -> bool {
    (key.code == KeyCode::BackTab || key.code == KeyCode::Tab)
        && key.modifiers == KeyModifiers::SHIFT
}
