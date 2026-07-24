use super::*;
use super::commit_log::seed_log_app;

fn named_commit(summary: &str) -> CommitEntry {
    CommitEntry::new(
        git2::Oid::ZERO_SHA1,
        "deadbee".to_string(),
        summary.to_string(),
        "T".to_string(),
        0,
    )
}

#[test]
fn commit_search_filters_summaries_and_clamps_selection() {
    let mut app = app_with_files(vec![]);
    app.mode = ViewMode::Log;
    app.log_view.set_commits(vec![
        named_commit("feat: search bar"),
        named_commit("docs: readme"),
        named_commit("fix: another search edge case"),
    ]);
    app.log_view.selected = 1;

    app.start_log_search();
    app.log_search_push('s');
    app.log_search_push('e');
    app.log_search_push('a');
    app.log_search_push('r');
    app.log_search_push('c');
    app.log_search_push('h');

    // "docs: readme" no longer matches → selection snaps to first match.
    assert_eq!(app.log_commit_filtered_indices(), &[0, 2]);
    assert_eq!(app.log_view.selected, 0);

    app.cancel_log_search();
    assert_eq!(app.log_commit_filtered_indices(), &[0, 1, 2]);
    assert!(app.log_view.commit_search_query.is_empty());
}

#[test]
fn maybe_prefetch_suppressed_while_commit_search_active() {
    // 10 loaded, threshold 5 — selected at 6 would normally spawn a fetch.
    let mut app = seed_log_app(10, 5, 5);
    app.log_view.selected = 6;
    app.log_view.commit_search_active = true;

    app.maybe_prefetch_commit_log();

    assert!(!app.log_view.pending_fetch);
    assert!(app.pagination.page_rx.is_none());
}

#[test]
fn cancel_log_search_resumes_prefetch() {
    let (dir, path) = make_repo();
    std::fs::write(Path::new(&path).join("a"), "x").unwrap();
    run_git(&path, &["add", "."]);
    run_git(&path, &["commit", "-m", "c"]);

    let mut app = seed_log_app(10, 5, 5);
    app.repo_path = path.clone();
    app.log_view.selected = 6;
    // Open the search bar so the prefetch gate is engaged.
    app.start_log_search();
    app.maybe_prefetch_commit_log();
    assert!(!app.log_view.pending_fetch);

    // Cancelling the search must re-call maybe_prefetch so the deferred
    // tail fetch can run now that the gate is lifted.
    app.cancel_log_search();
    assert!(app.log_view.pending_fetch);
    assert!(app.pagination.page_rx.is_some());

    let rx = app.pagination.page_rx.take().unwrap();
    let _ = rx.recv_timeout(Duration::from_secs(2)).unwrap();
    drop(dir);
}

#[test]
fn confirm_log_search_with_query_resumes_prefetch() {
    // Confirming (Enter) hides the bar but keeps the query — prefetch
    // must resume on the way out, mirroring the cancel path.
    let (dir, path) = make_repo();
    std::fs::write(Path::new(&path).join("a"), "x").unwrap();
    run_git(&path, &["add", "."]);
    run_git(&path, &["commit", "-m", "c"]);

    let mut app = seed_log_app(10, 5, 5);
    app.repo_path = path.clone();
    app.log_view.selected = 6;
    app.start_log_search();
    app.log_search_push('c'); // every fake summary matches.
    assert!(!app.log_view.pending_fetch);

    app.confirm_log_search();
    assert!(!app.log_view.commit_search_active);
    assert_eq!(app.log_view.commit_search_query.as_str(), "c");
    assert!(app.log_view.pending_fetch);

    let rx = app.pagination.page_rx.take().unwrap();
    let _ = rx.recv_timeout(Duration::from_secs(2)).unwrap();
    drop(dir);
}

#[test]
fn drilldown_file_search_filters_paths_and_clamps_selection() {
    let mut app = app_with_files(vec![]);
    app.mode = ViewMode::Log;
    app.log_view.drill_down = true;
    app.log_view.set_commit_files(vec![
        ChangedFile::unstaged_only("src/lib.rs".into(), StatusKind::Modified),
        ChangedFile::unstaged_only("README.md".into(), StatusKind::Modified),
        ChangedFile::unstaged_only("src/main.rs".into(), StatusKind::Modified),
    ]);
    app.log_view.file_selected = 1;

    app.start_log_search();
    app.log_search_push('s');
    app.log_search_push('r');
    app.log_search_push('c');

    // README.md drops out → selection snaps to first matching path.
    assert_eq!(app.log_file_filtered_indices(), &[0, 2]);
    assert_eq!(app.log_view.file_selected, 0);
    assert!(app.log_view.file_search_active);

    app.cancel_log_search();
    assert_eq!(app.log_file_filtered_indices(), &[0, 1, 2]);
    assert!(app.log_view.file_search_query.is_empty());
}