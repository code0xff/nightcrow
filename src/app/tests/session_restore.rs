use super::mode_toggle::seed_cached_commit_log;
use super::*;

fn repo_with_scrolled_diff() -> (tempfile::TempDir, String) {
    let (dir, path) = make_repo();
    let file = Path::new(&path).join("a.rs");
    std::fs::write(
        &file,
        (0..40).map(|n| format!("old {n}\n")).collect::<String>(),
    )
    .unwrap();
    run_git(&path, &["add", "."]);
    run_git(&path, &["commit", "-m", "base"]);
    std::fs::write(
        &file,
        (0..40).map(|n| format!("new {n}\n")).collect::<String>(),
    )
    .unwrap();
    (dir, path)
}

#[test]
fn status_restore_with_an_existing_list_applies_scroll_after_async_diff() {
    let (_dir, path) = repo_with_scrolled_diff();
    let mut app = app_with_files(vec!["a.rs"]);
    app.git.repo_path = path;

    app.restore_session(&crate::workspace::persistence::SessionState {
        selected_file: Some("a.rs".to_string()),
        scroll: 7,
        ..Default::default()
    });
    app.flush_git_loads_for_test(Duration::from_secs(2));

    assert_eq!(app.git.view.diff.scroll, 7);
}

#[test]
fn status_restore_from_first_snapshot_applies_scroll_after_async_diff() {
    let (_dir, path) = repo_with_scrolled_diff();
    let mut app = app_with_files(vec![]);
    app.git.repo_path = path;
    app.restore_session(&crate::workspace::persistence::SessionState {
        selected_file: Some("a.rs".to_string()),
        scroll: 7,
        ..Default::default()
    });

    app.ingest_snapshot(
        RepoSnapshot {
            files: vec![ChangedFile::unstaged_only(
                "a.rs".to_string(),
                StatusKind::Modified,
            )],
            tracking: None,
            head_oid: None,
            branch_name: None,
            refs_fingerprint: 0,
        },
        HashMap::new(),
    );
    app.flush_git_loads_for_test(Duration::from_secs(2));

    assert_eq!(app.git.view.diff.scroll, 7);
}

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
