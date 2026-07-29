use crate::app::{App, DiffPaneView, Focus, ViewMode};
use crate::application::input::dispatch::{
    KeyOutcome, ProjectRequest, matches_text_command, text_input_char,
};
use crate::input::{Action, encode_key, prefix_action, vim_navigation_action};
use crate::runtime::terminal::SCROLL_LINE_STEP;
use crate::workspace::Workspace;
use crossterm::event::{KeyCode, KeyEvent};

/// Keys on the empty screen: the leader arms, `o` opens the dialog, `q`
/// quits. Everything else is dropped — there is no project to act on and no
/// PTY to forward to.
pub(crate) fn handle_empty_key(ws: &mut Workspace, key: KeyEvent) -> KeyOutcome {
    if ws.prefix_armed() {
        ws.cancel_prefix();
        // `<L> <L>` sends a literal leader to the focused PTY on the project
        // screen; here there is no pane to send it to, so it is consumed.
        // Resolving it before the action table matters: with the default
        // `ctrl+f` leader the follow-up would otherwise match `f` and toggle
        // fullscreen.
        if ws.is_leader_key(key) {
            return KeyOutcome::Continue;
        }
        return match prefix_action(key) {
            Action::OpenProject => KeyOutcome::Project(ProjectRequest::OpenDialog),
            // Reachable with nothing open because the accent belongs to the
            // session rather than to a project: the empty screen is painted in
            // it too, and so is every other client that does have a tab up.
            Action::CycleTheme => KeyOutcome::Project(ProjectRequest::CycleAccent),
            Action::Quit => KeyOutcome::Quit,
            _ => KeyOutcome::Continue,
        };
    }
    if ws.is_leader_key(key) {
        ws.arm_prefix();
    }
    KeyOutcome::Continue
}

pub(crate) fn handle_terminal_key(app: &mut App, key: KeyEvent, action: Action) {
    match action {
        Action::TermScrollUp => {
            let lines = app.terminal.active_pane_rows();
            app.terminal.scroll_active(true, lines);
        }
        Action::TermScrollDown => {
            let lines = app.terminal.active_pane_rows();
            app.terminal.scroll_active(false, lines);
        }
        Action::TermScrollLineUp => app.terminal.scroll_active(true, SCROLL_LINE_STEP),
        Action::TermScrollLineDown => app.terminal.scroll_active(false, SCROLL_LINE_STEP),
        _ => {
            if let Some(data) = encode_key(key) {
                app.terminal.send_input(&data);
            }
        }
    }
}

pub(crate) fn handle_upper_key(app: &mut App, key: KeyEvent, action: Action) {
    if app.focus == Focus::FileList && app.status_view.search_active {
        handle_file_search_key(app, key);
        return;
    }
    if app.focus == Focus::FileList && app.tree_view.search_active {
        handle_tree_search_key(app, key);
        return;
    }
    if app.focus == Focus::FileList
        && (app.log_view.commit_search_active || app.log_view.file_search_active)
    {
        handle_log_search_key(app, key);
        return;
    }
    if app.focus == Focus::DiffViewer && app.diff.search.active {
        handle_diff_search_key(app, key);
        return;
    }

    // Apply vim-style j/k navigation only in upper panes; terminal focus is
    // routed through handle_terminal_key so j/k reach the PTY untouched.
    let action = vim_navigation_action(key).unwrap_or(action);

    match action {
        Action::Up => app.select_up(),
        Action::Down => app.select_down(),
        Action::PageUp => app.page_up(),
        Action::PageDown => app.page_down(),
        Action::TermScrollUp
        | Action::TermScrollDown
        | Action::TermScrollLineUp
        | Action::TermScrollLineDown => {}
        Action::None => handle_unmapped_upper_key(app, key),
        _ => {}
    }
}

fn handle_file_search_key(app: &mut App, key: KeyEvent) {
    match key.code {
        KeyCode::Up => app.select_up(),
        KeyCode::Down => app.select_down(),
        KeyCode::Esc => app.cancel_search(),
        KeyCode::Enter => app.confirm_search(),
        KeyCode::Backspace => {
            if app.status_view.search_query.is_empty() {
                app.cancel_search();
            } else {
                app.search_pop();
            }
        }
        _ => {
            // Reject command chords: Ctrl+letter reaches crossterm as the
            // literal letter, not as a control char, so modifier state is the
            // reliable guard against polluting the query.
            if let Some(c) = text_input_char(key) {
                app.search_push(c);
            }
        }
    }
}

fn handle_tree_search_key(app: &mut App, key: KeyEvent) {
    match key.code {
        KeyCode::Up => app.select_up(),
        KeyCode::Down => app.select_down(),
        KeyCode::Esc => app.cancel_tree_search(),
        KeyCode::Enter => app.confirm_tree_search(),
        KeyCode::Backspace => {
            if app.tree_view.search_query.is_empty() {
                app.cancel_tree_search();
            } else {
                app.tree_search_pop();
            }
        }
        _ => {
            // Same chord guard as the file search: Ctrl+letter arrives as the
            // bare letter, so modifier state is what keeps it out of the query.
            if let Some(c) = text_input_char(key) {
                app.tree_search_push(c);
            }
        }
    }
}

fn handle_log_search_key(app: &mut App, key: KeyEvent) {
    match key.code {
        KeyCode::Up => app.select_up(),
        KeyCode::Down => app.select_down(),
        KeyCode::Esc => app.cancel_log_search(),
        KeyCode::Enter => app.confirm_log_search(),
        KeyCode::Backspace => {
            // Which query is active depends on whether the drill-down file
            // list is showing; mirror the dispatch used by `log_search_push`.
            let query_empty = if app.log_view.drill_down {
                app.log_view.file_search_query.is_empty()
            } else {
                app.log_view.commit_search_query.is_empty()
            };
            if query_empty {
                app.cancel_log_search();
            } else {
                app.log_search_pop();
            }
        }
        _ => {
            if let Some(c) = text_input_char(key) {
                app.log_search_push(c);
            }
        }
    }
}

fn handle_diff_search_key(app: &mut App, key: KeyEvent) {
    match key.code {
        KeyCode::Esc => app.diff.cancel_search(),
        KeyCode::Enter => app.diff.confirm_search(),
        KeyCode::Backspace => {
            if app.diff.search.query.is_empty() {
                app.diff.cancel_search();
            } else {
                app.diff.search_pop();
            }
        }
        _ => {
            if let Some(c) = text_input_char(key) {
                app.diff.search_push(c);
            }
        }
    }
}

fn handle_unmapped_upper_key(app: &mut App, key: KeyEvent) {
    match app.focus {
        Focus::FileList => match key.code {
            KeyCode::Enter if app.mode == ViewMode::Log && !app.log_view.drill_down => {
                app.log_drill_in()
            }
            // Tree navigation: Enter toggles a directory (or re-previews a
            // file), Right expands, Left collapses / steps to the parent. These
            // guarded arms shadow the generic Left/Right horizontal-scroll arms
            // below while in Tree mode.
            KeyCode::Enter if app.mode == ViewMode::Tree => app.tree_toggle(),
            KeyCode::Right if app.mode == ViewMode::Tree => app.tree_expand(),
            KeyCode::Left if app.mode == ViewMode::Tree => app.tree_collapse(),
            // Log search Esc precedence sits ahead of `log_drill_out` so the
            // first Esc clears a confirmed filter before a second Esc exits
            // drill-down — mirrors the status-search Esc rule below.
            KeyCode::Esc
                if app.mode == ViewMode::Log
                    && app.log_view.drill_down
                    && !app.log_view.file_search_query.is_empty() =>
            {
                app.cancel_log_search()
            }
            KeyCode::Esc
                if app.mode == ViewMode::Log
                    && !app.log_view.drill_down
                    && !app.log_view.commit_search_query.is_empty() =>
            {
                app.cancel_log_search()
            }
            KeyCode::Esc if app.log_view.drill_down => app.log_drill_out(),
            _ if app.mode == ViewMode::Status && matches_text_command(key, '/') => {
                app.start_search()
            }
            _ if app.mode == ViewMode::Tree && matches_text_command(key, '/') => {
                app.start_tree_search()
            }
            _ if app.mode == ViewMode::Log && matches_text_command(key, '/') => {
                app.start_log_search()
            }
            KeyCode::Esc if !app.status_view.search_query.is_empty() => app.cancel_search(),
            KeyCode::Left => app.file_scroll_left(),
            KeyCode::Right => app.file_scroll_right(),
            _ => {}
        },
        Focus::DiffViewer => match key.code {
            _ if matches_text_command(key, 'v') => app.toggle_diff_file_view(),
            _ if matches_text_command(key, 's') => app.toggle_diff_split_view(),
            _ if matches_text_command(key, 'w') => app.toggle_diff_wrap(),
            // Walks all three views; `v`/`s` still jump straight to one.
            KeyCode::Tab => app.cycle_diff_view(),
            _ if matches_text_command(key, '/') => {
                exit_split_for_search(app);
                app.diff.start_search();
            }
            _ if matches_text_command(key, 'n') && app.diff.search.has_query() => {
                exit_split_for_search(app);
                app.diff.next_match();
            }
            _ if matches_text_command(key, 'N') && app.diff.search.has_query() => {
                exit_split_for_search(app);
                app.diff.prev_match();
            }
            KeyCode::Esc if !app.diff.search.query.is_empty() => app.diff.cancel_search(),
            KeyCode::Left => app.diff.scroll_left(),
            KeyCode::Right => app.diff.scroll_right(),
            _ => {}
        },
        Focus::Terminal => {}
    }
}

fn exit_split_for_search(app: &mut App) {
    if app.diff.view == DiffPaneView::Split {
        app.diff.view = DiffPaneView::Diff;
    }
}
