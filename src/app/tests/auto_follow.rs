use super::*;

fn snapshot_with(paths: &[&str]) -> RepoSnapshot {
    RepoSnapshot {
        files: paths
            .iter()
            .map(|p| ChangedFile::unstaged_only((*p).to_string(), StatusKind::Modified))
            .collect(),
        tracking: None,
        head_oid: None,
        branch_name: None,
    }
}

#[test]
fn ingest_snapshot_populates_hot_table_from_mtimes() {
    let mut app = app_with_files(vec![]);
    let snap = snapshot_with(&["a.rs", "b.rs"]);
    let now = SystemTime::now();
    let mtimes = HashMap::from([
        ("a.rs".to_string(), now),
        ("b.rs".to_string(), now - Duration::from_secs(5)),
    ]);

    app.ingest_snapshot(snap, mtimes);

    assert_eq!(app.status_view.hot_table.len(), 2);
    assert!(app.status_view.hot_table.contains_key("a.rs"));
    assert!(app.status_view.hot_table.contains_key("b.rs"));
}

#[test]
fn merge_hot_table_drops_paths_missing_from_new_snapshot() {
    let mut app = app_with_files(vec![]);
    let now = SystemTime::now();

    app.ingest_snapshot(
        snapshot_with(&["a.rs"]),
        HashMap::from([("a.rs".to_string(), now)]),
    );
    assert!(app.status_view.hot_table.contains_key("a.rs"));

    app.ingest_snapshot(snapshot_with(&["b.rs"]), HashMap::new());
    assert!(!app.status_view.hot_table.contains_key("a.rs"));
    assert!(!app.status_view.hot_table.contains_key("b.rs"));
}

#[test]
fn merge_hot_table_replaces_only_when_newer() {
    let mut app = app_with_files(vec![]);
    let old = SystemTime::UNIX_EPOCH + Duration::from_secs(100);
    let newer = SystemTime::UNIX_EPOCH + Duration::from_secs(200);

    app.ingest_snapshot(
        snapshot_with(&["a.rs"]),
        HashMap::from([("a.rs".to_string(), newer)]),
    );
    app.ingest_snapshot(
        snapshot_with(&["a.rs"]),
        HashMap::from([("a.rs".to_string(), old)]),
    );

    // The earlier mtime must not overwrite the newer observation; a
    // rename-from-stash scenario can resurrect older mtimes for the
    // same path and would otherwise demote a fresh edit to cool.
    assert_eq!(app.status_view.hot_table.get("a.rs"), Some(&newer));
}

#[test]
fn auto_follow_selects_freshest_hot_file_when_idle() {
    let mut app = app_with_files(vec!["a.rs", "b.rs"]);
    app.status_view.selected = 0;
    let now = SystemTime::now();

    app.ingest_snapshot(
        snapshot_with(&["a.rs", "b.rs"]),
        HashMap::from([
            ("a.rs".to_string(), now - Duration::from_secs(5)),
            ("b.rs".to_string(), now),
        ]),
    );

    // b.rs is fresher and the user is idle (last_manual_nav_at = None),
    // so selection must move from a.rs to b.rs.
    assert_eq!(app.status_view.selected, 1);
    assert_eq!(app.auto_follow.followed_path.as_deref(), Some("b.rs"));
}

#[test]
fn auto_follow_skipped_when_user_recently_navigated() {
    let mut app = app_with_files(vec!["a.rs", "b.rs"]);
    app.status_view.selected = 0;
    app.auto_follow.last_manual_nav_at = Some(Instant::now());
    let now = SystemTime::now();

    app.ingest_snapshot(
        snapshot_with(&["a.rs", "b.rs"]),
        HashMap::from([("b.rs".to_string(), now)]),
    );

    assert_eq!(app.status_view.selected, 0);
    assert!(app.auto_follow.followed_path.is_none());
}

#[test]
fn auto_follow_skipped_when_focus_not_filelist() {
    let mut app = app_with_files(vec!["a.rs", "b.rs"]);
    app.focus = Focus::DiffViewer;
    app.status_view.selected = 0;
    let now = SystemTime::now();

    app.ingest_snapshot(
        snapshot_with(&["a.rs", "b.rs"]),
        HashMap::from([("b.rs".to_string(), now)]),
    );

    assert_eq!(app.status_view.selected, 0);
    assert!(app.auto_follow.followed_path.is_none());
}

#[test]
fn auto_follow_skipped_when_disabled_in_config() {
    let mut app = app_with_files(vec!["a.rs", "b.rs"]);
    app.cfg_agent_indicator.auto_follow = false;
    app.status_view.selected = 0;
    let now = SystemTime::now();

    app.ingest_snapshot(
        snapshot_with(&["a.rs", "b.rs"]),
        HashMap::from([("b.rs".to_string(), now)]),
    );

    assert_eq!(app.status_view.selected, 0);
}

#[test]
fn auto_follow_skipped_when_freshest_is_already_selected() {
    let mut app = app_with_files(vec!["a.rs", "b.rs"]);
    app.status_view.selected = 1;
    let now = SystemTime::now();

    app.ingest_snapshot(
        snapshot_with(&["a.rs", "b.rs"]),
        HashMap::from([("b.rs".to_string(), now)]),
    );

    // Selection already points to b.rs — no need to steer or arm the
    // "already followed here" guard.
    assert_eq!(app.status_view.selected, 1);
    assert!(app.auto_follow.followed_path.is_none());
}

#[test]
fn select_down_marks_user_active_when_focus_is_filelist() {
    let mut app = app_with_files(vec!["a.rs", "b.rs"]);
    app.focus = Focus::FileList;
    app.auto_follow.followed_path = Some("a.rs".to_string());

    app.select_down();

    assert!(app.auto_follow.last_manual_nav_at.is_some());
    assert!(app.auto_follow.followed_path.is_none());
}

#[test]
fn select_down_does_not_mark_when_focus_is_diff() {
    let mut app = app_with_files(vec!["a.rs"]);
    app.focus = Focus::DiffViewer;

    app.select_down();

    assert!(app.auto_follow.last_manual_nav_at.is_none());
}

#[test]
fn auto_follow_respects_search_filter() {
    let mut app = app_with_files(vec!["alpha.rs", "beta.rs"]);
    app.status_view.search_query.set("alpha");
    app.status_view.recompute_filter();
    app.status_view.selected = 0; // alpha.rs (the only filtered entry)
    let now = SystemTime::now();

    app.ingest_snapshot(
        snapshot_with(&["alpha.rs", "beta.rs"]),
        HashMap::from([
            ("alpha.rs".to_string(), now - Duration::from_secs(5)),
            ("beta.rs".to_string(), now),
        ]),
    );

    // beta.rs is fresher but filtered out, so auto-follow must not
    // jump to a row the user cannot even see.
    assert_eq!(app.status_view.selected, 0);
}

#[test]
fn auto_follow_excludes_future_mtime_clock_skew() {
    // Regression for 962bde2: a file with mtime ahead of `now` (NFS
    // clock skew, files copied from a host with a wrong clock) used
    // to pin auto-follow forever because the inflated timestamp
    // beat every other candidate's `mtime > bm` comparison.
    // Future-stamped files must be excluded from consideration.
    let mut app = app_with_files(vec!["bogus.rs", "real.rs"]);
    app.status_view.selected = 0;
    let now = SystemTime::now();

    app.ingest_snapshot(
        snapshot_with(&["bogus.rs", "real.rs"]),
        HashMap::from([
            ("bogus.rs".to_string(), now + Duration::from_secs(3600)),
            ("real.rs".to_string(), now - Duration::from_secs(2)),
        ]),
    );

    // real.rs (the only candidate with a sane timestamp) must win,
    // and bogus.rs must not be recorded as the steered path.
    let real_idx = app
        .status_view
        .files
        .iter()
        .position(|f| f.path == "real.rs")
        .expect("real.rs must be in the file list");
    assert_eq!(app.status_view.selected, real_idx);
    assert_eq!(app.auto_follow.followed_path.as_deref(), Some("real.rs"));
}