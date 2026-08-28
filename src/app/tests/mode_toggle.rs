use super::*;

pub(super) fn seed_cached_commit_log(app: &mut App) {
    app.git.view.log.set_commits(vec![fake_entry(0)]);
    app.git.view.log.fully_loaded = true;
    app.set_observed_head_for_test(app.git.view.log.commits.first().map(|c| c.oid));
}

fn fake_entry(time: i64) -> CommitEntry {
    CommitEntry::new(
        git2::Oid::ZERO_SHA1,
        "deadbee".to_string(),
        format!("c{time}"),
        "T".to_string(),
        time,
    )
}

#[test]
fn toggle_mode_from_terminal_fullscreen_reveals_file_list() {
    let mut app = app_with_files(vec![]);
    seed_cached_commit_log(&mut app);
    app.terminal.panes = vec![PaneInfo {
        id: 1,
        title: "shell".into(),
    }];
    app.toggle_terminal_fullscreen();
    assert!(app.terminal.fullscreen.fills_body());

    app.toggle_mode();

    assert_eq!(app.git.view.mode, ViewMode::Log);
    assert!(!app.terminal.fullscreen.fills_body());
    assert!(!app.git.view.diff.fullscreen);
    assert_eq!(app.focus, Focus::FileList);
}

#[test]
fn toggle_mode_from_diff_fullscreen_reveals_file_list() {
    let mut app = app_with_files(vec![]);
    seed_cached_commit_log(&mut app);
    app.toggle_diff_fullscreen();
    assert!(app.git.view.diff.fullscreen);

    app.toggle_mode();

    assert_eq!(app.git.view.mode, ViewMode::Log);
    assert!(!app.git.view.diff.fullscreen);
    assert!(!app.terminal.fullscreen.fills_body());
    assert_eq!(app.focus, Focus::FileList);
}

#[test]
fn toggle_mode_in_split_layout_keeps_focus() {
    let mut app = app_with_files(vec![]);
    seed_cached_commit_log(&mut app);
    app.focus = Focus::DiffViewer;

    app.toggle_mode();

    assert_eq!(app.git.view.mode, ViewMode::Log);
    assert_eq!(app.focus, Focus::DiffViewer);

    app.toggle_mode();

    assert_eq!(app.git.view.mode, ViewMode::Status);
    assert_eq!(app.focus, Focus::DiffViewer);
}
