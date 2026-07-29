use super::helpers::*;
use crate::app::tests::app_with_files;
use crate::app::{Focus, ViewMode};
use crate::application::input::dispatch::handle_key;
use crossterm::event::{KeyCode, KeyModifiers};

#[test]
fn handle_key_leader_digits_mirror_focus_and_pane_fkeys() {
    // Digits mirror the no-prefix F-keys one-for-one: 1=F1 (file list),
    // 2=F2 (diff viewer), 3..9,0=F3..F10 (terminal panes 0..7). The
    // dispatcher consumes the digit (disarming the prefix) instead of
    // forwarding it to the PTY.
    let mut app = app_with_terminal_pane();
    app.terminal
        .create_pane_with_now(Some("echo two"), Some("two"))
        .unwrap();
    // Pad up to 8 panes so `<prefix> 0` (pane index 7) below is a real
    // switch, not a no-op against an out-of-range index.
    for i in 2..8 {
        app.terminal
            .create_pane_with_now(None, Some(&format!("pane{i}")))
            .unwrap();
    }
    assert_eq!(app.terminal.panes.len(), 8);
    app.switch_pane(0);

    // <prefix> 1 → focus file list (mirrors F1)
    let _ = handle_key(&mut app, leader());
    let _ = handle_key(&mut app, press(KeyCode::Char('1'), KeyModifiers::NONE));
    assert_eq!(app.focus, Focus::FileList, "leader+1 must mirror F1");

    // <prefix> 2 → focus diff viewer (mirrors F2)
    let _ = handle_key(&mut app, leader());
    let _ = handle_key(&mut app, press(KeyCode::Char('2'), KeyModifiers::NONE));
    assert_eq!(app.focus, Focus::DiffViewer, "leader+2 must mirror F2");

    // <prefix> 4 → terminal pane 1 (mirrors F4)
    let _ = handle_key(&mut app, leader());
    let _ = handle_key(&mut app, press(KeyCode::Char('4'), KeyModifiers::NONE));
    assert_eq!(app.terminal.active, 1, "leader+4 must mirror F4 → pane 1");

    // <prefix> 0 → terminal pane 7 (mirrors F10)
    let _ = handle_key(&mut app, leader());
    let _ = handle_key(&mut app, press(KeyCode::Char('0'), KeyModifiers::NONE));
    assert_eq!(app.terminal.active, 7, "leader+0 must mirror F10 → pane 7");

    assert!(
        !app.prefix_armed(),
        "a mapped follow-up must disarm the prefix"
    );
    assert!(
        backend_payloads(&app).is_empty(),
        "a consumed leader digit must not reach the PTY"
    );
}

#[test]
fn handle_key_leader_b_toggles_tree_mode() {
    // `<prefix> b` enters Tree mode and a second `<prefix> b` returns to
    // Status. Uses the live cwd repo (the crate root) for the root read.
    let mut app = app_with_files(vec!["a.rs"]);
    app.focus = Focus::FileList;
    assert_eq!(app.mode, ViewMode::Status);

    let _ = handle_key(&mut app, leader());
    let _ = handle_key(&mut app, press(KeyCode::Char('b'), KeyModifiers::NONE));
    assert_eq!(app.mode, ViewMode::Tree);

    let _ = handle_key(&mut app, leader());
    let _ = handle_key(&mut app, press(KeyCode::Char('b'), KeyModifiers::NONE));
    assert_eq!(app.mode, ViewMode::Status);
}

#[test]
fn handle_key_tree_right_left_expand_and_collapse() {
    let (dir, path) = crate::test_util::make_repo();
    let root = std::path::Path::new(&path);
    std::fs::create_dir(root.join("sub")).unwrap();
    std::fs::write(root.join("sub").join("f.txt"), "x").unwrap();

    let mut app = app_with_files(vec![]);
    app.repo_path = path.clone();
    app.focus = Focus::FileList;
    app.enter_tree_mode();
    let idx = app
        .tree_view
        .visible_rows()
        .iter()
        .position(|r| r.path == "sub")
        .unwrap();
    app.tree_view.selected = idx;

    // Right expands the directory.
    let _ = handle_key(&mut app, press(KeyCode::Right, KeyModifiers::NONE));
    assert!(
        app.tree_view
            .visible_rows()
            .iter()
            .any(|r| r.path == "sub/f.txt"),
        "Right must expand the selected directory"
    );

    // Left collapses it again.
    let _ = handle_key(&mut app, press(KeyCode::Left, KeyModifiers::NONE));
    assert!(
        !app.tree_view
            .visible_rows()
            .iter()
            .any(|r| r.path == "sub/f.txt"),
        "Left must collapse the expanded directory"
    );
    drop(dir);
}
