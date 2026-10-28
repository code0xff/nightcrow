use super::*;

fn status_app(path: &str) -> App {
    let mut app = app_with_files(vec!["a.rs", "b.rs"]);
    app.git.repo_path = path.to_string();
    app
}

fn repo_with_two_dirty_files() -> (tempfile::TempDir, String) {
    let (dir, path) = make_repo();
    std::fs::write(Path::new(&path).join("a.rs"), "old a\n").unwrap();
    std::fs::write(Path::new(&path).join("b.rs"), "old b\n").unwrap();
    run_git(&path, &["add", "."]);
    run_git(&path, &["commit", "-m", "base"]);
    std::fs::write(Path::new(&path).join("a.rs"), "latest a\n").unwrap();
    std::fs::write(Path::new(&path).join("b.rs"), "latest b\n").unwrap();
    (dir, path)
}

fn diff_text(app: &App) -> String {
    app.git
        .view
        .diff
        .hunks()
        .iter()
        .flat_map(|hunk| hunk.lines.iter())
        .map(|line| line.content.as_str())
        .collect::<Vec<_>>()
        .join("\n")
}

#[test]
fn one_hundred_thousand_consecutive_selections_do_not_block_the_input_loop_and_apply_the_last_diff()
{
    let (_dir, path) = repo_with_two_dirty_files();
    let mut app = status_app(&path);
    let started = Instant::now();

    for _ in 0..50_000 {
        app.select_down();
        app.select_up();
    }

    let input_latency = started.elapsed();
    eprintln!("100k selection input loop: {input_latency:?}");
    assert!(
        input_latency < Duration::from_secs(5),
        "100k selections blocked for {:?}",
        input_latency
    );
    app.flush_git_loads_for_test(Duration::from_secs(5));
    assert!(diff_text(&app).contains("latest a"));
    assert!(!diff_text(&app).contains("latest b"));
}

#[test]
fn results_from_the_previous_repository_are_discarded_after_a_switch() {
    let (_old_dir, old_path) = repo_with_two_dirty_files();
    let (_new_dir, new_path) = repo_with_two_dirty_files();
    std::fs::write(Path::new(&new_path).join("a.rs"), "new repo only\n").unwrap();
    let mut app = status_app(&old_path);

    app.reload_diff();
    app.git.repo_path = new_path;
    app.reload_diff();
    app.flush_git_loads_for_test(Duration::from_secs(5));

    assert!(diff_text(&app).contains("new repo only"));
    assert!(
        app.notice
            .as_ref()
            .is_none_or(|notice| notice.kind != NoticeKind::Diff)
    );
}

#[test]
fn consecutive_commit_selections_apply_only_the_last_oid_diff_and_title() {
    let (_dir, path) = make_repo();
    let file = Path::new(&path).join("a.rs");
    std::fs::write(&file, "one\n").unwrap();
    run_git(&path, &["add", "."]);
    run_git(&path, &["commit", "-m", "first"]);
    std::fs::write(&file, "two\n").unwrap();
    run_git(&path, &["add", "."]);
    run_git(&path, &["commit", "-m", "second"]);
    let mut app = app_with_files(vec![]);
    app.git.repo_path = path.clone();
    app.git.view.mode = ViewMode::Log;
    app.git
        .view
        .log
        .set_commits(load_commit_log(&open_repo(&path), 10).unwrap());

    app.git.view.log.selected = 0;
    app.load_commit_diff_for_selected();
    app.git.view.log.selected = 1;
    app.load_commit_diff_for_selected();
    app.flush_git_loads_for_test(Duration::from_secs(5));

    assert!(app.git.view.log.diff_title.contains("first"));
    assert!(!app.git.view.log.diff_title.contains("second"));
    assert!(diff_text(&app).contains("one"));
}

#[test]
fn asynchronous_refresh_preserves_diff_search_and_scroll() {
    let (_dir, path) = make_repo();
    let file = Path::new(&path).join("a.rs");
    std::fs::write(&file, "zero\none\ntwo\nthree\n").unwrap();
    run_git(&path, &["add", "."]);
    run_git(&path, &["commit", "-m", "base"]);
    std::fs::write(&file, "zero\nneedle\ntwo changed\nthree\n").unwrap();
    let mut app = app_with_files(vec!["a.rs"]);
    app.git.repo_path = path;
    app.git
        .view
        .diff
        .set_hunks(vec![context_hunk(&["old", "old", "old"])]);
    app.git.view.diff.scroll = 2;
    app.git.view.diff.search.query.set("needle");
    let old_mtime = SystemTime::UNIX_EPOCH + Duration::from_secs(1);
    let new_mtime = SystemTime::UNIX_EPOCH + Duration::from_secs(2);
    app.git
        .view
        .status
        .hot_table
        .insert("a.rs".into(), old_mtime);

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
            operation: None,
            conflicts: 0,
        },
        HashMap::from([("a.rs".to_string(), new_mtime)]),
    );
    app.flush_git_loads_for_test(Duration::from_secs(5));

    assert_eq!(app.git.view.diff.search.query.as_str(), "needle");
    assert!(!app.git.view.diff.search.matches.is_empty());
    assert_eq!(
        app.git.view.diff.scroll,
        2.min(app.git.view.diff.max_scroll())
    );
}

#[test]
fn file_view_opened_after_diff_is_not_closed_by_the_diff_reply() {
    let (_dir, path) = repo_with_two_dirty_files();
    let mut app = status_app(&path);

    app.reload_diff();
    app.toggle_diff_file_view();
    app.flush_git_loads_for_test(Duration::from_secs(5));

    assert_eq!(app.git.view.diff.view, DiffPaneView::File);
    assert_eq!(
        app.git.view.diff.file_view.key,
        Some(FileViewKey::Status("a.rs".to_string()))
    );
    assert_eq!(app.git.view.diff.file_view.content, "latest a\n");
}

#[test]
fn selection_change_does_not_preserve_the_previous_file_view() {
    let (_dir, path) = repo_with_two_dirty_files();
    let mut app = status_app(&path);

    app.reload_diff();
    app.toggle_diff_file_view();
    app.flush_git_loads_for_test(Duration::from_secs(5));
    assert_eq!(
        app.git.view.diff.file_view.key,
        Some(FileViewKey::Status("a.rs".to_string()))
    );

    app.select_down();
    app.flush_git_loads_for_test(Duration::from_secs(5));

    assert_eq!(app.selected_filtered_status_path().as_deref(), Some("b.rs"));
    assert_eq!(app.git.view.diff.view, DiffPaneView::Diff);
    assert_eq!(app.git.view.diff.file_view.key, None);
    assert!(diff_text(&app).contains("latest b"));
}

#[test]
fn mode_switch_drops_a_stale_commit_files_reply() {
    let (_dir, path) = make_repo();
    std::fs::write(Path::new(&path).join("a.rs"), "one\n").unwrap();
    run_git(&path, &["add", "."]);
    run_git(&path, &["commit", "-m", "first"]);
    let mut app = app_with_files(vec!["a.rs"]);
    app.git.repo_path = path.clone();
    app.git.view.mode = ViewMode::Log;
    app.git
        .view
        .log
        .set_commits(load_commit_log(&open_repo(&path), 10).unwrap());

    app.log_drill_in();
    app.toggle_mode();
    app.flush_git_loads_for_test(Duration::from_secs(5));

    assert_eq!(app.git.view.mode, ViewMode::Status);
    assert!(!app.git.view.log.drill_down);
}

#[test]
fn worker_failure_for_the_current_selection_is_shown_as_a_diff_notice() {
    let mut app = app_with_files(vec!["a.rs"]);
    app.git.repo_path = "repository-that-does-not-exist".to_string();

    app.reload_diff();
    app.flush_git_loads_for_test(Duration::from_secs(5));

    assert_eq!(
        app.notice.as_ref().map(|notice| notice.kind),
        Some(NoticeKind::Diff)
    );
}
