use super::*;
use super::mode_toggle::seed_cached_commit_log;

#[test]
fn toggle_mode_in_list_fullscreen_keeps_list_fullscreen() {
    let mut app = app_with_files(vec![]);
    seed_cached_commit_log(&mut app);
    app.toggle_list_fullscreen();
    assert!(app.list_fullscreen);

    app.toggle_mode();

    assert_eq!(app.mode, ViewMode::Log);
    assert!(app.list_fullscreen);
    assert_eq!(app.focus, Focus::FileList);
}

#[test]
fn toggle_list_fullscreen_exits_terminal_fullscreen() {
    let mut app = app_with_files(vec![]);
    app.terminal.panes = vec![PaneInfo {
        id: 1,
        title: "shell".into(),
    }];
    app.toggle_terminal_fullscreen();
    assert!(app.terminal.fullscreen.fills_body());

    app.toggle_list_fullscreen();

    assert!(app.list_fullscreen);
    assert!(!app.terminal.fullscreen.fills_body());
    assert_eq!(app.focus, Focus::FileList);
}

#[test]
fn toggle_diff_fullscreen_exits_list_fullscreen() {
    let mut app = app_with_files(vec![]);
    app.toggle_list_fullscreen();
    assert!(app.list_fullscreen);

    app.toggle_diff_fullscreen();

    assert!(app.diff.fullscreen);
    assert!(!app.list_fullscreen);
    assert_eq!(app.focus, Focus::DiffViewer);
}

#[test]
fn toggle_terminal_fullscreen_exits_list_fullscreen() {
    let mut app = app_with_files(vec![]);
    app.terminal.panes = vec![PaneInfo {
        id: 1,
        title: "shell".into(),
    }];
    app.toggle_list_fullscreen();
    assert!(app.list_fullscreen);

    app.toggle_terminal_fullscreen();

    assert!(app.terminal.fullscreen.fills_body());
    assert!(!app.list_fullscreen);
    assert_eq!(app.focus, Focus::Terminal);
}

#[test]
fn cycle_focus_is_noop_in_list_fullscreen() {
    let mut app = app_with_files(vec![]);
    app.terminal.panes = vec![PaneInfo {
        id: 1,
        title: "shell".into(),
    }];
    app.toggle_list_fullscreen();
    assert_eq!(app.focus, Focus::FileList);

    app.cycle_focus_forward();
    assert_eq!(app.focus, Focus::FileList);

    app.cycle_focus_backward();
    assert_eq!(app.focus, Focus::FileList);
}

#[test]
fn switch_pane_exits_list_fullscreen() {
    let mut app = app_with_files(vec![]);
    app.terminal.panes = vec![PaneInfo {
        id: 1,
        title: "shell".into(),
    }];
    app.toggle_list_fullscreen();
    assert!(app.list_fullscreen);

    app.switch_pane(0);

    assert!(!app.list_fullscreen);
    assert_eq!(app.focus, Focus::Terminal);
}

#[test]
fn save_session_round_trips_list_fullscreen() {
    let mut app = app_with_files(vec![]);
    app.toggle_list_fullscreen();
    assert!(app.list_fullscreen);

    let state = app.save_session();
    assert!(state.list_fullscreen);

    let mut other = app_with_files(vec![]);
    other.restore_session(&state);
    assert!(other.list_fullscreen);
    assert_eq!(other.focus, Focus::FileList);
}

#[test]
fn restore_session_list_fullscreen_forces_filelist_focus() {
    let mut app = app_with_files(vec![]);

    app.restore_session(&crate::session::SessionState {
        focus: Some(Focus::DiffViewer),
        list_fullscreen: true,
        ..Default::default()
    });

    assert!(app.list_fullscreen);
    assert_eq!(app.focus, Focus::FileList);
}

#[test]
fn restore_session_prefers_terminal_fullscreen_over_list_fullscreen() {
    let mut app = app_with_files(vec![]);
    app.terminal.panes = vec![PaneInfo {
        id: 1,
        title: "shell".into(),
    }];

    app.restore_session(&crate::session::SessionState {
        focus: Some(Focus::FileList),
        terminal_fullscreen: true,
        list_fullscreen: true,
        ..Default::default()
    });

    assert!(app.terminal.fullscreen.fills_body());
    assert!(!app.list_fullscreen);
    assert_eq!(app.focus, Focus::Terminal);
}

#[test]
fn close_last_pane_exits_fullscreen() {
    let mut app = app_with_files(vec![]);
    app.terminal.panes = vec![PaneInfo {
        id: 1,
        title: "shell".into(),
    }];
    app.terminal.fullscreen = TerminalFullscreen::Grid;
    app.focus = Focus::Terminal;
    app.terminal.scroll.insert(1, 3);
    app.terminal.prompt_bufs.insert(1, "cargo test".to_string());
    app.terminal
        .emulators
        .insert(1, crate::runtime::emulator::PaneEmulator::new(3, 10, 0));

    app.close_active_pane();

    assert!(!app.terminal.fullscreen.fills_body());
    assert_eq!(app.focus, Focus::DiffViewer);
    assert!(!app.terminal.scroll.contains_key(&1));
    assert!(!app.terminal.prompt_bufs.contains_key(&1));
    assert!(!app.terminal.emulators.contains_key(&1));
}

#[test]
fn restore_session_restores_active_pane_even_when_focus_is_not_terminal() {
    let mut app = app_with_files(vec![]);
    app.terminal.panes = vec![
        PaneInfo { id: 1, title: "shell 1".into() },
        PaneInfo { id: 2, title: "shell 2".into() },
    ];

    app.restore_session(&crate::session::SessionState {
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

    app.restore_session(&crate::session::SessionState {
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

    app.restore_session(&crate::session::SessionState {
        focus: Some(Focus::FileList),
        diff_fullscreen: true,
        ..Default::default()
    });

    assert!(app.diff.fullscreen);
    assert_eq!(app.focus, Focus::DiffViewer);
}

#[test]
fn restore_session_prefers_terminal_fullscreen_over_diff_fullscreen() {
    let mut app = app_with_files(vec![]);
    app.terminal.panes = vec![PaneInfo {
        id: 1,
        title: "shell".into(),
    }];

    app.restore_session(&crate::session::SessionState {
        focus: Some(Focus::FileList),
        terminal_fullscreen: true,
        diff_fullscreen: true,
        ..Default::default()
    });

    assert!(app.terminal.fullscreen.fills_body());
    assert!(!app.diff.fullscreen);
    assert_eq!(app.focus, Focus::Terminal);
}

#[test]
fn save_session_round_trips_diff_fullscreen() {
    let mut app = app_with_files(vec![]);
    app.toggle_diff_fullscreen();
    assert!(app.diff.fullscreen);

    let state = app.save_session();
    assert!(state.diff_fullscreen);

    let mut other = app_with_files(vec![]);
    other.restore_session(&state);
    assert!(other.diff.fullscreen);
    assert_eq!(other.focus, Focus::DiffViewer);
}

#[test]
fn restore_session_normalizes_accent_index() {
    let mut app = app_with_files(vec![]);

    app.restore_session(&crate::session::SessionState {
        accent_idx: usize::MAX,
        ..Default::default()
    });

    assert_eq!(
        app.accent_idx,
        usize::MAX % crate::config::Accent::ALL.len()
    );
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
    app.repo_path = path;

    app.restore_session(&crate::session::SessionState {
        mode: Some(ViewMode::Log),
        scroll: 2,
        ..Default::default()
    });

    assert_eq!(app.mode, ViewMode::Log);
    assert!(!app.diff.hunks.is_empty());
    assert_eq!(app.diff.scroll, 2);
}