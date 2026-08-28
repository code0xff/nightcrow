use super::*;

fn status_app(path: &str) -> App {
    let mut app = app_with_files(vec!["a.rs", "b.rs"]);
    app.repo_path = path.to_string();
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
    app.diff
        .hunks()
        .iter()
        .flat_map(|hunk| hunk.lines.iter())
        .map(|line| line.content.as_str())
        .collect::<Vec<_>>()
        .join("\n")
}

#[test]
fn 십만번_연속_선택은_입력_loop를_block하지_않고_마지막_diff를_적용한다() {
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
fn 저장소가_바뀐_뒤_도착한_이전_저장소_결과는_버린다() {
    let (_old_dir, old_path) = repo_with_two_dirty_files();
    let (_new_dir, new_path) = repo_with_two_dirty_files();
    std::fs::write(Path::new(&new_path).join("a.rs"), "new repo only\n").unwrap();
    let mut app = status_app(&old_path);

    app.reload_diff();
    app.repo_path = new_path;
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
fn 연속_commit_선택은_마지막_oid의_diff와_title만_적용한다() {
    let (_dir, path) = make_repo();
    let file = Path::new(&path).join("a.rs");
    std::fs::write(&file, "one\n").unwrap();
    run_git(&path, &["add", "."]);
    run_git(&path, &["commit", "-m", "first"]);
    std::fs::write(&file, "two\n").unwrap();
    run_git(&path, &["add", "."]);
    run_git(&path, &["commit", "-m", "second"]);
    let mut app = app_with_files(vec![]);
    app.repo_path = path.clone();
    app.mode = ViewMode::Log;
    app.log_view
        .set_commits(load_commit_log(&open_repo(&path), 10).unwrap());

    app.log_view.selected = 0;
    app.load_commit_diff_for_selected();
    app.log_view.selected = 1;
    app.load_commit_diff_for_selected();
    app.flush_git_loads_for_test(Duration::from_secs(5));

    assert!(app.log_view.diff_title.contains("first"));
    assert!(!app.log_view.diff_title.contains("second"));
    assert!(diff_text(&app).contains("one"));
}

#[test]
fn 비동기_새로고침은_diff_검색과_scroll을_보존한다() {
    let (_dir, path) = make_repo();
    let file = Path::new(&path).join("a.rs");
    std::fs::write(&file, "zero\none\ntwo\nthree\n").unwrap();
    run_git(&path, &["add", "."]);
    run_git(&path, &["commit", "-m", "base"]);
    std::fs::write(&file, "zero\nneedle\ntwo changed\nthree\n").unwrap();
    let mut app = app_with_files(vec!["a.rs"]);
    app.repo_path = path;
    app.diff
        .set_hunks(vec![context_hunk(&["old", "old", "old"])]);
    app.diff.scroll = 2;
    app.diff.search.query.set("needle");
    let old_mtime = SystemTime::UNIX_EPOCH + Duration::from_secs(1);
    let new_mtime = SystemTime::UNIX_EPOCH + Duration::from_secs(2);
    app.status_view.hot_table.insert("a.rs".into(), old_mtime);

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
        HashMap::from([("a.rs".to_string(), new_mtime)]),
    );
    app.flush_git_loads_for_test(Duration::from_secs(5));

    assert_eq!(app.diff.search.query.as_str(), "needle");
    assert!(!app.diff.search.matches.is_empty());
    assert_eq!(app.diff.scroll, 2.min(app.diff.max_scroll()));
}

#[test]
fn diff보다_나중에_연_file_view는_diff_reply가_닫지_않는다() {
    let (_dir, path) = repo_with_two_dirty_files();
    let mut app = status_app(&path);

    app.reload_diff();
    app.toggle_diff_file_view();
    app.flush_git_loads_for_test(Duration::from_secs(5));

    assert_eq!(app.diff.view, DiffPaneView::File);
    assert_eq!(
        app.diff.file_view.key,
        Some(FileViewKey::Status("a.rs".to_string()))
    );
    assert_eq!(app.diff.file_view.content, "latest a\n");
}

#[test]
fn selection_change_does_not_preserve_the_previous_file_view() {
    let (_dir, path) = repo_with_two_dirty_files();
    let mut app = status_app(&path);

    app.reload_diff();
    app.toggle_diff_file_view();
    app.flush_git_loads_for_test(Duration::from_secs(5));
    assert_eq!(
        app.diff.file_view.key,
        Some(FileViewKey::Status("a.rs".to_string()))
    );

    app.select_down();
    app.flush_git_loads_for_test(Duration::from_secs(5));

    assert_eq!(app.selected_filtered_status_path().as_deref(), Some("b.rs"));
    assert_eq!(app.diff.view, DiffPaneView::Diff);
    assert_eq!(app.diff.file_view.key, None);
    assert!(diff_text(&app).contains("latest b"));
}

#[test]
fn mode_switch_drops_a_stale_commit_files_reply() {
    let (_dir, path) = make_repo();
    std::fs::write(Path::new(&path).join("a.rs"), "one\n").unwrap();
    run_git(&path, &["add", "."]);
    run_git(&path, &["commit", "-m", "first"]);
    let mut app = app_with_files(vec!["a.rs"]);
    app.repo_path = path.clone();
    app.mode = ViewMode::Log;
    app.log_view
        .set_commits(load_commit_log(&open_repo(&path), 10).unwrap());

    app.log_drill_in();
    app.toggle_mode();
    app.flush_git_loads_for_test(Duration::from_secs(5));

    assert_eq!(app.mode, ViewMode::Status);
    assert!(!app.log_view.drill_down);
}

#[test]
fn 현재_선택의_worker_실패는_diff_notice로_남는다() {
    let mut app = app_with_files(vec!["a.rs"]);
    app.repo_path = "repository-that-does-not-exist".to_string();

    app.reload_diff();
    app.flush_git_loads_for_test(Duration::from_secs(5));

    assert_eq!(
        app.notice.as_ref().map(|notice| notice.kind),
        Some(NoticeKind::Diff)
    );
}
