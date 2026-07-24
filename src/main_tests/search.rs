use super::helpers::*;
use crate::app::{DiffPaneView, Focus};
use crate::app::tests::app_with_files;
use crate::key_dispatch::handle_key;
use crossterm::event::{KeyCode, KeyModifiers};

#[test]
fn handle_key_overlay_blocks_leader_when_diff_search_active() {
    // While a search overlay is open the leader is typed/consumed by the
    // overlay, never arming the prefix or firing an app command.
    let mut app = app_with_files(vec!["a.rs"]);
    app.focus = Focus::DiffViewer;
    app.diff.start_search();
    assert!(app.diff.search.active);
    let before = app.mode;

    let _ = handle_key(&mut app, leader());
    assert!(!app.prefix_armed(), "leader must not arm behind an overlay");
    let _ = handle_key(&mut app, press(KeyCode::Char('l'), KeyModifiers::NONE));

    assert_eq!(
        app.mode, before,
        "no app command may fire behind an overlay"
    );
    assert!(app.diff.search.active, "diff search must remain open");
}

#[test]
fn handle_key_file_search_rejects_command_modifier_chars() {
    let mut app = app_with_files(vec!["a.rs"]);
    app.focus = Focus::FileList;
    app.start_search();

    let ctrl_x = press(KeyCode::Char('x'), KeyModifiers::CONTROL);
    let _ = handle_key(&mut app, ctrl_x);

    assert!(app.status_view.search_query.is_empty());
}

#[test]
fn handle_key_diff_search_rejects_command_modifier_chars() {
    let mut app = app_with_files(vec!["a.rs"]);
    app.focus = Focus::DiffViewer;
    app.diff.start_search();

    let alt_x = press(KeyCode::Char('x'), KeyModifiers::ALT);
    let _ = handle_key(&mut app, alt_x);

    assert!(app.diff.search.query.is_empty());
}

#[test]
fn handle_key_status_search_shortcut_requires_no_command_modifier() {
    let mut app = app_with_files(vec!["a.rs"]);
    app.focus = Focus::FileList;

    let ctrl_slash = press(KeyCode::Char('/'), KeyModifiers::CONTROL);
    let _ = handle_key(&mut app, ctrl_slash);

    assert!(!app.status_view.search_active);
}

#[test]
fn handle_key_diff_file_toggle_requires_no_command_modifier() {
    let mut app = app_with_files(vec!["a.rs"]);
    app.focus = Focus::DiffViewer;

    let alt_v = press(KeyCode::Char('v'), KeyModifiers::ALT);
    let _ = handle_key(&mut app, alt_v);

    assert_eq!(app.diff.view, DiffPaneView::Diff);
}

#[test]
fn handle_key_diff_search_from_split_returns_to_unified_overlay() {
    let mut app = app_with_files(vec!["a.rs"]);
    app.focus = Focus::DiffViewer;
    app.diff.view = DiffPaneView::Split;

    let _ = handle_key(&mut app, press(KeyCode::Char('/'), KeyModifiers::NONE));

    assert_eq!(app.diff.view, DiffPaneView::Diff);
    assert!(app.diff.search.active);
}

#[test]
fn handle_key_diff_next_match_from_split_returns_to_unified_when_query_exists() {
    let mut app = app_with_files(vec!["a.rs"]);
    app.focus = Focus::DiffViewer;
    app.diff.view = DiffPaneView::Split;
    app.diff.search.query.set("needle");

    let _ = handle_key(&mut app, press(KeyCode::Char('n'), KeyModifiers::NONE));

    assert_eq!(app.diff.view, DiffPaneView::Diff);
}
