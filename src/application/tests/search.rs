use super::helpers::*;
use crate::app::tests::app_with_files;
use crate::app::{DiffPaneView, Focus};
use crate::application::input::dispatch::handle_key;
use crossterm::event::{KeyCode, KeyModifiers};

#[test]
fn handle_key_overlay_blocks_leader_when_diff_search_active() {
    // While a search overlay is open the leader is typed/consumed by the
    // overlay, never arming the prefix or firing an app command.
    let mut app = app_with_files(vec!["a.rs"]);
    app.focus = Focus::DiffViewer;
    app.git.view.diff.start_search();
    assert!(app.git.view.diff.search.active);
    let before = app.git.view.mode;

    let _ = handle_key(&mut app, leader());
    assert!(
        !app.interaction.prefix_armed,
        "leader must not arm behind an overlay"
    );
    let _ = handle_key(&mut app, press(KeyCode::Char('l'), KeyModifiers::NONE));

    assert_eq!(
        app.git.view.mode, before,
        "no app command may fire behind an overlay"
    );
    assert!(
        app.git.view.diff.search.active,
        "diff search must remain open"
    );
}

#[test]
fn handle_key_file_search_rejects_command_modifier_chars() {
    let mut app = app_with_files(vec!["a.rs"]);
    app.focus = Focus::FileList;
    app.start_search();

    let ctrl_x = press(KeyCode::Char('x'), KeyModifiers::CONTROL);
    let _ = handle_key(&mut app, ctrl_x);

    assert!(app.git.view.status.search_query.is_empty());
}

#[test]
fn handle_key_diff_search_rejects_command_modifier_chars() {
    let mut app = app_with_files(vec!["a.rs"]);
    app.focus = Focus::DiffViewer;
    app.git.view.diff.start_search();

    let alt_x = press(KeyCode::Char('x'), KeyModifiers::ALT);
    let _ = handle_key(&mut app, alt_x);

    assert!(app.git.view.diff.search.query.is_empty());
}

#[test]
fn handle_key_status_search_shortcut_requires_no_command_modifier() {
    let mut app = app_with_files(vec!["a.rs"]);
    app.focus = Focus::FileList;

    let ctrl_slash = press(KeyCode::Char('/'), KeyModifiers::CONTROL);
    let _ = handle_key(&mut app, ctrl_slash);

    assert!(!app.git.view.status.search_active);
}

#[test]
fn handle_key_diff_file_toggle_requires_no_command_modifier() {
    let mut app = app_with_files(vec!["a.rs"]);
    app.focus = Focus::DiffViewer;

    let alt_v = press(KeyCode::Char('v'), KeyModifiers::ALT);
    let _ = handle_key(&mut app, alt_v);

    assert_eq!(app.git.view.diff.view, DiffPaneView::Diff);
}

#[test]
fn handle_key_diff_search_from_split_returns_to_unified_overlay() {
    let mut app = app_with_files(vec!["a.rs"]);
    app.focus = Focus::DiffViewer;
    app.git.view.diff.view = DiffPaneView::Split;

    let _ = handle_key(&mut app, press(KeyCode::Char('/'), KeyModifiers::NONE));

    assert_eq!(app.git.view.diff.view, DiffPaneView::Diff);
    assert!(app.git.view.diff.search.active);
}

#[test]
fn handle_key_diff_next_match_from_split_returns_to_unified_when_query_exists() {
    let mut app = app_with_files(vec!["a.rs"]);
    app.focus = Focus::DiffViewer;
    app.git.view.diff.view = DiffPaneView::Split;
    app.git.view.diff.search.query.set("needle");

    let _ = handle_key(&mut app, press(KeyCode::Char('n'), KeyModifiers::NONE));

    assert_eq!(app.git.view.diff.view, DiffPaneView::Diff);
}

#[test]
fn tab_in_the_diff_viewer_cycles_the_view() {
    // Tab reaching the diff viewer at all is the point: it is not a text
    // command, so it needs its own arm in the focus handler.
    let mut app = app_with_files(vec!["a.rs"]);
    app.focus = Focus::DiffViewer;
    assert_eq!(app.git.view.diff.view, DiffPaneView::Diff);

    let _ = handle_key(&mut app, press(KeyCode::Tab, KeyModifiers::NONE));

    assert_eq!(app.git.view.diff.view, DiffPaneView::Split);
}

#[test]
fn tab_outside_the_diff_viewer_leaves_the_view_alone() {
    // The file list owns Tab-less navigation and the terminal forwards Tab to
    // its PTY; neither may reach the diff pane's cycle.
    let mut app = app_with_files(vec!["a.rs"]);
    app.focus = Focus::FileList;

    let _ = handle_key(&mut app, press(KeyCode::Tab, KeyModifiers::NONE));

    assert_eq!(app.git.view.diff.view, DiffPaneView::Diff);
}
