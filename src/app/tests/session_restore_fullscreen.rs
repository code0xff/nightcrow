//! Restoring the parts of a session that point at a pane.
//!
//! Which pane was active, whether the panel was fullscreen, where the focus was:
//! all of it needs a pane to point at, so it waits for the session to report one
//! (see `App::pending_terminal`). These drive that arrival.

use super::*;

#[test]
fn restore_session_restores_active_pane_even_when_focus_is_not_terminal() {
    let mut app = app_with_files(vec![]);
    app.terminal.panes = vec![
        PaneInfo {
            id: 1,
            title: "shell 1".into(),
        },
        PaneInfo {
            id: 2,
            title: "shell 2".into(),
        },
    ];

    app.restore_session(&crate::workspace::persistence::SessionState {
        focus: Some(Focus::FileList),
        active_pane: 1,
        ..Default::default()
    });

    assert_eq!(app.focus, Focus::FileList);
    assert_eq!(app.terminal.active, 1);
}

#[test]
fn restore_session_fullscreen_forces_terminal_focus() {
    let mut app = app_with_files(vec![]);
    app.terminal.panes = vec![PaneInfo {
        id: 1,
        title: "shell".into(),
    }];

    app.restore_session(&crate::workspace::persistence::SessionState {
        focus: Some(Focus::FileList),
        terminal_fullscreen: true,
        ..Default::default()
    });

    assert!(app.terminal.fullscreen.fills_body());
    assert_eq!(app.focus, Focus::Terminal);
}

#[test]
fn restore_session_diff_fullscreen_forces_diff_focus() {
    let mut app = app_with_files(vec![]);

    app.restore_session(&crate::workspace::persistence::SessionState {
        focus: Some(Focus::FileList),
        diff_fullscreen: true,
        ..Default::default()
    });

    assert!(app.git.view.diff.fullscreen);
    assert_eq!(app.focus, Focus::DiffViewer);
}

#[test]
fn restore_session_prefers_terminal_fullscreen_over_diff_fullscreen() {
    let mut app = app_with_files(vec![]);
    app.terminal.panes = vec![PaneInfo {
        id: 1,
        title: "shell".into(),
    }];

    app.restore_session(&crate::workspace::persistence::SessionState {
        focus: Some(Focus::FileList),
        terminal_fullscreen: true,
        diff_fullscreen: true,
        ..Default::default()
    });

    assert!(app.terminal.fullscreen.fills_body());
    assert!(!app.git.view.diff.fullscreen);
    assert_eq!(app.focus, Focus::Terminal);
}

#[test]
fn save_session_round_trips_diff_fullscreen() {
    let mut app = app_with_files(vec![]);
    app.toggle_diff_fullscreen();
    assert!(app.git.view.diff.fullscreen);

    let state = app.save_session();
    assert!(state.diff_fullscreen);

    let mut other = app_with_files(vec![]);
    other.restore_session(&state);
    assert!(other.git.view.diff.fullscreen);
    assert_eq!(other.focus, Focus::DiffViewer);
}

#[test]
fn restore_session_keeps_log_scroll_after_loading_commit_diff() {
    let (_dir, path) = make_repo();
    let file_path = Path::new(&path).join("a.rs");
    std::fs::write(
        &file_path,
        "fn main() {\n    println!(\"one\");\n    println!(\"two\");\n}\n",
    )
    .unwrap();
    run_git(&path, &["add", "."]);
    run_git(&path, &["commit", "-m", "init"]);

    let mut app = app_with_files(vec![]);
    app.git.repo_path = path;

    app.restore_session(&crate::workspace::persistence::SessionState {
        mode: Some(ViewMode::Log),
        scroll: 2,
        ..Default::default()
    });
    app.flush_git_loads_for_test(Duration::from_secs(2));

    assert_eq!(app.git.view.mode, ViewMode::Log);
    assert!(!app.git.view.diff.hunks().is_empty());
    assert_eq!(app.git.view.diff.scroll, 2);
}
