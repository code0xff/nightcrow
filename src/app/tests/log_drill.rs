use super::*;

#[test]
fn log_drill_in_clears_stale_diff_for_empty_commit() {
    let (_dir, path) = make_repo();
    run_git(&path, &["commit", "--allow-empty", "-m", "empty"]);

    let mut app = app_with_files(vec![]);
    app.repo_path = path.clone();
    app.mode = ViewMode::Log;
    app.log_view
        .set_commits(load_commit_log(&open_repo(&path), 1).unwrap());
    app.diff.hunks = vec![context_hunk(&["stale"])];
    app.log_view.diff_title = "stale".to_string();

    app.log_drill_in();

    assert!(app.log_view.drill_down);
    assert!(app.log_view.commit_files.is_empty());
    assert!(app.diff.hunks.is_empty());
    assert!(app.log_view.diff_title.contains("empty"));
}
