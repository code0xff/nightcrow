use super::*;

fn fake_entry(time: i64) -> CommitEntry {
    CommitEntry::new(
        git2::Oid::ZERO_SHA1,
        "deadbee".to_string(),
        format!("c{time}"),
        "T".to_string(),
        time,
    )
}

pub(super) fn seed_log_app(entries: usize, page_size: usize, threshold: usize) -> App {
    let mut app = app_with_files(vec![]);
    app.mode = ViewMode::Log;
    app.configure_commit_log(page_size, threshold);
    let commits: Vec<_> = (0..entries).map(|i| fake_entry(i as i64)).collect();
    app.log_view.set_commits(commits);
    app
}

#[test]
fn maybe_prefetch_no_ops_in_status_mode() {
    let mut app = seed_log_app(10, 5, 5);
    app.mode = ViewMode::Status;
    app.log_view.selected = 9;

    app.maybe_prefetch_commit_log();

    assert!(!app.log_view.pending_fetch);
    assert!(!app.commit_log_fetch_pending());
}

#[test]
fn maybe_prefetch_no_ops_when_empty() {
    let mut app = seed_log_app(0, 5, 5);
    app.maybe_prefetch_commit_log();
    assert!(!app.log_view.pending_fetch);
    assert!(!app.commit_log_fetch_pending());
}

#[test]
fn maybe_prefetch_no_ops_when_fully_loaded() {
    let mut app = seed_log_app(10, 5, 5);
    app.log_view.fully_loaded = true;
    app.log_view.selected = 9;

    app.maybe_prefetch_commit_log();

    assert!(!app.log_view.pending_fetch);
    assert!(!app.commit_log_fetch_pending());
}

#[test]
fn maybe_prefetch_no_ops_when_far_from_tail() {
    // 10 loaded, threshold 3 — selected at 5 is 5 rows from tail, no prefetch.
    let mut app = seed_log_app(10, 5, 3);
    app.log_view.selected = 5;

    app.maybe_prefetch_commit_log();

    assert!(!app.log_view.pending_fetch);
    assert!(!app.commit_log_fetch_pending());
}

#[test]
fn maybe_prefetch_triggers_when_near_tail() {
    // 10 loaded, threshold 5 — selected at 6 is within 4 rows of the tail.
    let (dir, path) = make_repo();
    std::fs::write(Path::new(&path).join("a"), "x").unwrap();
    run_git(&path, &["add", "."]);
    run_git(&path, &["commit", "-m", "c"]);
    let mut app = seed_log_app(10, 5, 5);
    app.repo_path = path.clone();
    app.log_view.selected = 6;

    app.maybe_prefetch_commit_log();

    assert!(app.log_view.pending_fetch);
    assert!(app.commit_log_fetch_pending());

    // Wait for the worker to land so its result doesn't leak into a
    // subsequent test scenario.
    app.flush_commit_log_fetch_for_test(Duration::from_secs(2));
    drop(dir);
}

#[test]
fn maybe_prefetch_suppresses_duplicate_pending() {
    let (dir, path) = make_repo();
    std::fs::write(Path::new(&path).join("a"), "x").unwrap();
    run_git(&path, &["add", "."]);
    run_git(&path, &["commit", "-m", "c"]);
    let mut app = seed_log_app(10, 5, 5);
    app.repo_path = path.clone();
    app.log_view.selected = 6;

    app.maybe_prefetch_commit_log();
    assert!(app.commit_log_fetch_pending());

    app.maybe_prefetch_commit_log();
    assert!(app.commit_log_fetch_pending());

    app.flush_commit_log_fetch_for_test(Duration::from_secs(2));
    drop(dir);
}

#[test]
fn worker_reply_appends_matching_tail() {
    let (_dir, path) = make_repo();
    run_git(&path, &["commit", "--allow-empty", "-m", "c0"]);
    run_git(&path, &["commit", "--allow-empty", "-m", "c1"]);
    let mut app = seed_log_app(0, 1, 1);
    app.repo_path = path;
    app.spawn_commit_log_page_fetch(0);
    app.flush_commit_log_fetch_for_test(Duration::from_secs(2));
    assert_eq!(app.log_view.commits.len(), 2);
    assert_eq!(app.log_view.loaded_count, 2);
}

#[test]
fn worker_reply_discards_stale_tail() {
    let (_dir, path) = make_repo();
    run_git(&path, &["commit", "--allow-empty", "-m", "c0"]);
    let mut app = seed_log_app(1, 1, 1);
    app.repo_path = path;
    app.spawn_commit_log_page_fetch(1);
    app.log_view
        .set_commits(vec![fake_entry(9), fake_entry(10)]);
    app.flush_commit_log_fetch_for_test(Duration::from_secs(2));
    assert_eq!(app.log_view.commits.len(), 2);
    assert_eq!(app.log_view.commits[0].summary, "c9");
}

#[test]
fn refresh_after_head_change_prepends_new_commit() {
    let (dir, path) = make_repo();
    std::fs::write(Path::new(&path).join("a"), "1").unwrap();
    run_git(&path, &["add", "."]);
    run_git(&path, &["commit", "-m", "c0"]);
    std::fs::write(Path::new(&path).join("a"), "2").unwrap();
    run_git(&path, &["add", "."]);
    run_git(&path, &["commit", "-m", "c1"]);

    let mut app = app_with_files(vec![]);
    app.repo_path = path.clone();
    app.mode = ViewMode::Log;
    // Simulate having loaded the commit list when c0 was still HEAD.
    app.log_view
        .set_commits(load_commit_log(&open_repo(&path), 500).unwrap()[1..].to_vec());
    let prior_oid = app.log_view.commits.first().unwrap().oid;
    assert_eq!(app.log_view.commits.len(), 1);
    app.log_view.selected = 0;

    app.refresh_commit_log_after_head_change();
    app.flush_commit_log_fetch_for_test(Duration::from_secs(2));

    // The fresh c1 commit is prepended; selection shifts so the user keeps
    // looking at c0.
    assert_eq!(app.log_view.commits.len(), 2);
    assert_eq!(app.log_view.commits[1].oid, prior_oid);
    assert_eq!(app.log_view.selected, 1);
    drop(dir);
}

#[test]
fn refresh_after_head_change_keeps_merged_side_branch_commits() {
    let (dir, path) = make_repo();
    std::fs::write(Path::new(&path).join("base"), "0").unwrap();
    run_git(&path, &["add", "."]);
    run_git(&path, &["commit", "-m", "c0"]);

    run_git(&path, &["checkout", "-b", "feature"]);
    std::fs::write(Path::new(&path).join("feature"), "feature").unwrap();
    run_git(&path, &["add", "."]);
    run_git(&path, &["commit", "-m", "feature"]);

    run_git(&path, &["checkout", "-"]);
    std::fs::write(Path::new(&path).join("main"), "main").unwrap();
    run_git(&path, &["add", "."]);
    run_git(&path, &["commit", "-m", "c1"]);

    let mut app = app_with_files(vec![]);
    app.repo_path = path.clone();
    app.mode = ViewMode::Log;
    app.log_view
        .set_commits(load_commit_log(&open_repo(&path), 500).unwrap());
    assert_eq!(app.log_view.commits.len(), 2);
    assert_eq!(app.log_view.commits[0].summary, "c1");

    run_git(
        &path,
        &["merge", "--no-ff", "feature", "-m", "merge feature"],
    );

    app.refresh_commit_log_after_head_change();
    app.flush_commit_log_fetch_for_test(Duration::from_secs(2));

    let summaries: Vec<_> = app
        .log_view
        .commits
        .iter()
        .map(|c| c.summary.as_str())
        .collect();
    assert!(
        summaries.contains(&"feature"),
        "merged side-branch commit was dropped: {summaries:?}"
    );
    assert_eq!(app.log_view.commits.len(), 4);
    drop(dir);
}

#[test]
fn refresh_after_head_change_resets_on_divergence() {
    let (dir, path) = make_repo();
    std::fs::write(Path::new(&path).join("a"), "1").unwrap();
    run_git(&path, &["add", "."]);
    run_git(&path, &["commit", "-m", "c0"]);

    let mut app = app_with_files(vec![]);
    app.repo_path = path.clone();
    app.mode = ViewMode::Log;
    // Pretend a prior list whose head no longer exists in the repo —
    // simulates rebase/reset/branch switch that drops the old chain.
    let ghost_oid = git2::Oid::from_str("0123456789abcdef0123456789abcdef01234567").unwrap();
    app.log_view.set_commits(vec![CommitEntry::new(
        ghost_oid,
        "012345".to_string(),
        "vanished".to_string(),
        "T".to_string(),
        0,
    )]);
    app.log_view.selected = 0;

    app.refresh_commit_log_after_head_change();
    app.flush_commit_log_fetch_for_test(Duration::from_secs(2));

    // c0 from the actual repo replaces the ghost entry.
    assert_eq!(app.log_view.commits.len(), 1);
    assert_ne!(app.log_view.commits[0].oid, ghost_oid);
    assert_eq!(app.log_view.selected, 0);
    drop(dir);
}
