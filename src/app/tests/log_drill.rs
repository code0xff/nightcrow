use super::*;

#[test]
fn log_drill_in_clears_stale_diff_for_empty_commit() {
    let (_dir, path) = make_repo();
    run_git(&path, &["commit", "--allow-empty", "-m", "empty"]);

    let mut app = app_with_files(vec![]);
    app.git.repo_path = path.clone();
    app.git.view.mode = ViewMode::Log;
    app.git
        .view
        .log
        .set_commits(load_commit_log(&open_repo(&path), 1).unwrap());
    app.git.view.diff.set_hunks(vec![context_hunk(&["stale"])]);
    app.git.view.log.diff_title = "stale".to_string();

    app.log_drill_in();
    app.flush_git_loads_for_test(Duration::from_secs(2));

    assert!(app.git.view.log.drill_down);
    assert!(app.git.view.log.commit_files.is_empty());
    assert!(app.git.view.diff.hunks().is_empty());
    assert!(app.git.view.log.diff_title.contains("empty"));
}
