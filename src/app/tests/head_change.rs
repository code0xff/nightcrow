use super::*;

/// Helper: build a snapshot tied to the given repo so HEAD-change detection
/// has a real oid to compare against. The snapshot itself is otherwise
/// empty — we only care about `head_oid` in these tests.
fn snapshot_with_head(repo_path: &str) -> RepoSnapshot {
    let head = open_repo(repo_path).head().ok().and_then(|h| h.target());
    RepoSnapshot {
        files: Vec::new(),
        tracking: None,
        head_oid: head,
        branch_name: None,
        refs_fingerprint: 0,
    }
}

#[test]
fn head_change_in_log_mode_reloads_commit_list() {
    let (_dir, path) = make_repo();
    run_git(&path, &["commit", "--allow-empty", "-m", "first"]);
    run_git(&path, &["commit", "--allow-empty", "-m", "second"]);

    let (snapshot, tx) = dummy_snapshot_channel();
    let mut app = app_with_files(vec![]);
    app.git.snapshot = snapshot;
    app.git.repo_path = path.clone();
    app.git.view.mode = ViewMode::Log;
    app.git
        .view
        .log
        .set_commits(load_commit_log(&open_repo(&path), 500).unwrap());
    app.git.view.log.selected = 0;
    app.set_observed_head_for_test(app.git.view.log.commits.first().map(|c| c.oid));
    assert_eq!(app.git.view.log.commits.len(), 2);

    // Make a new commit in the same repo (simulates the terminal pane
    // running `git commit`).
    run_git(&path, &["commit", "--allow-empty", "-m", "third"]);

    tx.send(SnapshotMsg::Ok(snapshot_with_head(&path), HashMap::new()))
        .unwrap();
    app.poll_snapshot();
    app.flush_commit_log_fetch_for_test(Duration::from_secs(2));

    assert_eq!(
        app.git.view.log.commits.len(),
        3,
        "commit list should auto-refresh on HEAD change"
    );
    assert_eq!(app.git.view.log.commits[0].summary, "third");
}

#[test]
fn head_change_in_status_mode_does_not_reload() {
    let (_dir, path) = make_repo();
    run_git(&path, &["commit", "--allow-empty", "-m", "first"]);

    let (snapshot, tx) = dummy_snapshot_channel();
    let mut app = app_with_files(vec![]);
    app.git.snapshot = snapshot;
    app.git.repo_path = path.clone();
    // Pre-load a stale 1-entry list; in Status mode it must NOT be
    // refreshed even when HEAD moves.
    app.git
        .view
        .log
        .set_commits(load_commit_log(&open_repo(&path), 500).unwrap());
    app.set_observed_head_for_test(app.git.view.log.commits.first().map(|c| c.oid));
    assert_eq!(app.git.view.log.commits.len(), 1);
    assert_eq!(app.git.view.mode, ViewMode::Status);

    run_git(&path, &["commit", "--allow-empty", "-m", "second"]);

    tx.send(SnapshotMsg::Ok(snapshot_with_head(&path), HashMap::new()))
        .unwrap();
    app.poll_snapshot();

    assert_eq!(
        app.git.view.log.commits.len(),
        1,
        "Status mode must not eagerly refresh the (hidden) commit list"
    );
}

#[test]
fn toggling_log_after_status_head_change_reloads_stale_cache() {
    let (_dir, path) = make_repo();
    run_git(&path, &["commit", "--allow-empty", "-m", "first"]);

    let (snapshot, tx) = dummy_snapshot_channel();
    let mut app = app_with_files(vec![]);
    app.git.snapshot = snapshot;
    app.git.repo_path = path.clone();
    app.git.view.mode = ViewMode::Status;
    app.git
        .view
        .log
        .set_commits(load_commit_log(&open_repo(&path), 500).unwrap());
    app.set_observed_head_for_test(app.git.view.log.commits.first().map(|c| c.oid));
    assert_eq!(app.git.view.log.commits[0].summary, "first");

    run_git(&path, &["commit", "--allow-empty", "-m", "second"]);
    tx.send(SnapshotMsg::Ok(snapshot_with_head(&path), HashMap::new()))
        .unwrap();
    app.poll_snapshot();

    // Status mode leaves the hidden list untouched, but records the new
    // HEAD. Entering Log mode must notice the mismatch and reconcile page 0
    // rather than reusing the stale cached page as-is.
    assert_eq!(app.git.view.log.commits.len(), 1);
    assert_eq!(app.git.view.log.commits[0].summary, "first");

    app.toggle_mode();
    app.flush_commit_log_fetch_for_test(Duration::from_secs(2));

    assert_eq!(app.git.view.mode, ViewMode::Log);
    assert_eq!(app.git.view.log.commits.len(), 2);
    assert_eq!(app.git.view.log.commits[0].summary, "second");
    assert_eq!(app.git.view.log.selected, 1);
    assert_eq!(
        app.git.view.log.commits[app.git.view.log.selected].summary,
        "first"
    );
    assert!(app.git.view.log.fully_loaded);
    assert!(!app.commit_log_fetch_pending());
}

#[test]
fn head_change_preserves_selected_commit_by_oid() {
    let (_dir, path) = make_repo();
    run_git(&path, &["commit", "--allow-empty", "-m", "first"]);
    run_git(&path, &["commit", "--allow-empty", "-m", "second"]);

    let (snapshot, tx) = dummy_snapshot_channel();
    let mut app = app_with_files(vec![]);
    app.git.snapshot = snapshot;
    app.git.repo_path = path.clone();
    app.git.view.mode = ViewMode::Log;
    app.git
        .view
        .log
        .set_commits(load_commit_log(&open_repo(&path), 500).unwrap());
    // Select the older commit at the bottom.
    app.git.view.log.selected = 1;
    let prior_oid = app.git.view.log.commits[1].oid;
    app.set_observed_head_for_test(app.git.view.log.commits.first().map(|c| c.oid));

    run_git(&path, &["commit", "--allow-empty", "-m", "third"]);

    tx.send(SnapshotMsg::Ok(snapshot_with_head(&path), HashMap::new()))
        .unwrap();
    app.poll_snapshot();
    app.flush_commit_log_fetch_for_test(Duration::from_secs(2));

    // The 'first' commit now sits at index 2 because a new commit is
    // prepended. Selection must follow it by oid, not by index.
    assert_eq!(app.git.view.log.commits.len(), 3);
    assert_eq!(app.git.view.log.selected, 2);
    assert_eq!(
        app.git.view.log.commits[app.git.view.log.selected].oid,
        prior_oid
    );
}

#[test]
fn head_change_falls_back_to_top_when_prior_oid_gone() {
    let (_dir, path) = make_repo();
    run_git(&path, &["commit", "--allow-empty", "-m", "first"]);
    run_git(&path, &["commit", "--allow-empty", "-m", "second"]);

    let (snapshot, tx) = dummy_snapshot_channel();
    let mut app = app_with_files(vec![]);
    app.git.snapshot = snapshot;
    app.git.repo_path = path.clone();
    app.git.view.mode = ViewMode::Log;
    app.git
        .view
        .log
        .set_commits(load_commit_log(&open_repo(&path), 500).unwrap());
    app.git.view.log.selected = 0;
    app.set_observed_head_for_test(app.git.view.log.commits.first().map(|c| c.oid));

    // Reset to before the second commit so the prior HEAD oid is gone,
    // then add a different commit on top.
    run_git(&path, &["reset", "--hard", "HEAD~1"]);
    run_git(&path, &["commit", "--allow-empty", "-m", "other"]);

    tx.send(SnapshotMsg::Ok(snapshot_with_head(&path), HashMap::new()))
        .unwrap();
    app.poll_snapshot();
    app.flush_commit_log_fetch_for_test(Duration::from_secs(2));

    // The original selected commit ('second') no longer exists; selection
    // must fall back to the newest (index 0).
    assert_eq!(app.git.view.log.selected, 0);
    assert_eq!(app.git.view.log.commits[0].summary, "other");
}

#[test]
fn head_change_clears_drill_down_when_commit_gone() {
    let (_dir, path) = make_repo();
    run_git(&path, &["commit", "--allow-empty", "-m", "first"]);
    run_git(&path, &["commit", "--allow-empty", "-m", "doomed"]);

    let (snapshot, tx) = dummy_snapshot_channel();
    let mut app = app_with_files(vec![]);
    app.git.snapshot = snapshot;
    app.git.repo_path = path.clone();
    app.git.view.mode = ViewMode::Log;
    app.git
        .view
        .log
        .set_commits(load_commit_log(&open_repo(&path), 500).unwrap());
    app.git.view.log.selected = 0; // 'doomed' commit at top
    app.git.view.log.drill_down = true;
    app.set_observed_head_for_test(app.git.view.log.commits.first().map(|c| c.oid));

    // Drop the selected commit via reset, then advance HEAD with a new one.
    run_git(&path, &["reset", "--hard", "HEAD~1"]);
    run_git(&path, &["commit", "--allow-empty", "-m", "replacement"]);

    tx.send(SnapshotMsg::Ok(snapshot_with_head(&path), HashMap::new()))
        .unwrap();
    app.poll_snapshot();
    app.flush_commit_log_fetch_for_test(Duration::from_secs(2));

    // The drill-down's commit oid is gone, so drill-down must collapse
    // and the view drops back to the commit-level diff.
    assert!(!app.git.view.log.drill_down);
}

#[test]
fn initial_snapshot_does_not_trigger_commit_log_reload() {
    let (_dir, path) = make_repo();
    run_git(&path, &["commit", "--allow-empty", "-m", "first"]);

    let (snapshot, tx) = dummy_snapshot_channel();
    let mut app = app_with_files(vec![]);
    app.git.snapshot = snapshot;
    app.git.repo_path = path.clone();
    app.git.view.mode = ViewMode::Log;
    // No prior commits loaded; last_head_oid = None (default).
    assert!(app.git.view.log.commits.is_empty());
    assert!(app.observed_head_for_test().is_none());

    tx.send(SnapshotMsg::Ok(snapshot_with_head(&path), HashMap::new()))
        .unwrap();
    app.poll_snapshot();

    // First snapshot must NOT eagerly fetch the commit log — that's
    // toggle_mode's / restore_log_session's job. We only refresh on
    // subsequent HEAD changes.
    assert!(app.git.view.log.commits.is_empty());
    assert!(app.observed_head_for_test().is_some());
}
