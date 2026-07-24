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
    app.pagination.page_size = page_size;
    app.pagination.prefetch_threshold = threshold;
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
    assert!(app.pagination.page_rx.is_none());
}

#[test]
fn maybe_prefetch_no_ops_when_empty() {
    let mut app = seed_log_app(0, 5, 5);
    app.maybe_prefetch_commit_log();
    assert!(!app.log_view.pending_fetch);
    assert!(app.pagination.page_rx.is_none());
}

#[test]
fn maybe_prefetch_no_ops_when_fully_loaded() {
    let mut app = seed_log_app(10, 5, 5);
    app.log_view.fully_loaded = true;
    app.log_view.selected = 9;

    app.maybe_prefetch_commit_log();

    assert!(!app.log_view.pending_fetch);
    assert!(app.pagination.page_rx.is_none());
}

#[test]
fn maybe_prefetch_no_ops_when_far_from_tail() {
    // 10 loaded, threshold 3 — selected at 5 is 5 rows from tail, no prefetch.
    let mut app = seed_log_app(10, 5, 3);
    app.log_view.selected = 5;

    app.maybe_prefetch_commit_log();

    assert!(!app.log_view.pending_fetch);
    assert!(app.pagination.page_rx.is_none());
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
    assert!(app.pagination.page_rx.is_some());

    // Wait for the worker to land so its result doesn't leak into a
    // subsequent test scenario.
    let rx = app.pagination.page_rx.take().unwrap();
    let _ = rx.recv_timeout(Duration::from_secs(2)).unwrap();
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
    let first_rx_ptr = app.pagination.page_rx.as_ref().map(|r| r as *const _);
    assert!(first_rx_ptr.is_some());

    app.maybe_prefetch_commit_log();
    let second_rx_ptr = app.pagination.page_rx.as_ref().map(|r| r as *const _);
    // The second call must reuse the same receiver — no second spawn.
    assert_eq!(first_rx_ptr, second_rx_ptr);

    let rx = app.pagination.page_rx.take().unwrap();
    let _ = rx.recv_timeout(Duration::from_secs(2)).unwrap();
    drop(dir);
}

#[test]
fn poll_drains_matching_skip_into_commits() {
    let mut app = seed_log_app(3, 5, 1);
    app.log_view.pending_fetch = true;
    let (tx, rx) = mpsc::channel();
    app.pagination.page_rx = Some(rx);
    // Worker thinks the loaded tail was 3 when it ran; this matches.
    tx.send(CommitLogPageMsg {
        kind: CommitLogFetchKind::Tail,
        skip: 3,
        page_size: 5,
        result: Ok(vec![fake_entry(3), fake_entry(4)]),
    })
    .unwrap();

    app.poll_commit_log_page_fetch();

    assert_eq!(app.log_view.commits.len(), 5);
    assert_eq!(app.log_view.loaded_count, 5);
    // Page was shorter than page_size → end of history reached.
    assert!(app.log_view.fully_loaded);
    assert!(!app.log_view.pending_fetch);
    assert!(app.pagination.page_rx.is_none());
}

#[test]
fn poll_discards_stale_skip_result() {
    let mut app = seed_log_app(3, 5, 1);
    app.log_view.pending_fetch = true;
    let (tx, rx) = mpsc::channel();
    app.pagination.page_rx = Some(rx);
    // skip=2 doesn't match loaded_count=3 → discard (e.g. HEAD changed
    // between spawn and reply, resetting pagination).
    tx.send(CommitLogPageMsg {
        kind: CommitLogFetchKind::Tail,
        skip: 2,
        page_size: 5,
        result: Ok(vec![fake_entry(2), fake_entry(3)]),
    })
    .unwrap();

    app.poll_commit_log_page_fetch();

    assert_eq!(app.log_view.commits.len(), 3);
    assert!(!app.log_view.fully_loaded);
    assert!(!app.log_view.pending_fetch);
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