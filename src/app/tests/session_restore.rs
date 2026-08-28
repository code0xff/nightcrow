use super::mode_toggle::seed_cached_commit_log;
use super::*;

#[test]
fn toggle_mode_in_list_fullscreen_keeps_list_fullscreen() {
    let mut app = app_with_files(vec![]);
    seed_cached_commit_log(&mut app);
    app.toggle_list_fullscreen();
    assert!(app.list_fullscreen);

    app.toggle_mode();

    assert_eq!(app.git.view.mode, ViewMode::Log);
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

    assert!(app.git.view.diff.fullscreen);
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

    app.restore_session(&crate::workspace::persistence::SessionState {
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

    app.restore_session(&crate::workspace::persistence::SessionState {
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
    // Through a backend, because closing is a request now: a state with none has
    // nobody to ask, and the pane it was handed is not its to remove.
    let mut app = app_with_fake_backend();
    app.terminal.create_pane_now().unwrap();
    let pane = app.terminal.panes[0].id;
    app.terminal.fullscreen = TerminalFullscreen::Grid;
    app.focus = Focus::Terminal;
    app.terminal.scroll.insert(pane, 3);
    app.terminal
        .prompt_bufs
        .insert(pane, "cargo test".to_string());

    app.close_active_pane();
    app.poll_terminal();

    assert!(!app.terminal.fullscreen.fills_body());
    assert_eq!(app.focus, Focus::DiffViewer);
    assert!(!app.terminal.scroll.contains_key(&pane));
    assert!(!app.terminal.prompt_bufs.contains_key(&pane));
    assert!(!app.terminal.emulators.contains_key(&pane));
}
