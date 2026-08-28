use super::*;

#[test]
fn successful_snapshot_preserves_terminal_status() {
    let (snapshot, tx) = dummy_snapshot_channel();
    let mut app = App {
        notice: Some(Notice::new(NoticeKind::Terminal, "backend unavailable")),
        snapshot,
        pending_snapshot: None,
        ..app_with_files(vec![])
    };

    tx.send(SnapshotMsg::Ok(
        RepoSnapshot {
            files: Vec::new(),
            tracking: None,
            head_oid: None,
            branch_name: None,
            refs_fingerprint: 0,
        },
        HashMap::new(),
    ))
    .unwrap();
    app.poll_snapshot();

    assert_eq!(
        app.notice,
        Some(Notice::new(NoticeKind::Terminal, "backend unavailable"))
    );
}

#[test]
fn successful_snapshot_clears_git_status() {
    let (snapshot, tx) = dummy_snapshot_channel();
    let mut app = App {
        notice: Some(Notice::new(NoticeKind::Git, "not a repo")),
        snapshot,
        pending_snapshot: None,
        ..app_with_files(vec![])
    };

    tx.send(SnapshotMsg::Ok(
        RepoSnapshot {
            files: Vec::new(),
            tracking: None,
            head_oid: None,
            branch_name: None,
            refs_fingerprint: 0,
        },
        HashMap::new(),
    ))
    .unwrap();
    app.poll_snapshot();

    assert_eq!(app.notice, None);
}

#[test]
fn snapshot_refresh_clamps_selection_to_active_filter() {
    let (snapshot, tx) = dummy_snapshot_channel();
    let mut app = App {
        snapshot,
        pending_snapshot: None,
        ..app_with_files(vec!["bar.rs"])
    };
    app.status_view.search_query.set("bar");
    app.status_view.recompute_filter();

    tx.send(SnapshotMsg::Ok(
        RepoSnapshot {
            files: vec![
                ChangedFile::unstaged_only("aaa.rs".to_string(), StatusKind::Modified),
                ChangedFile::unstaged_only("bar2.rs".to_string(), StatusKind::Modified),
            ],
            tracking: None,
            head_oid: None,
            branch_name: None,
            refs_fingerprint: 0,
        },
        HashMap::new(),
    ))
    .unwrap();
    app.poll_snapshot();

    assert_eq!(app.filtered_indices(), &[1]);
    assert_eq!(app.status_view.selected, 1);
    assert_eq!(
        app.status_view.files[app.status_view.selected].path,
        "bar2.rs"
    );
}

#[test]
fn snapshot_invalidates_path_width_cache_on_same_length_rename() {
    let (snapshot, tx) = dummy_snapshot_channel();
    let mut app = App {
        snapshot,
        pending_snapshot: None,
        ..app_with_files(vec!["short.rs"])
    };
    // Prime the width cache by reading the right-scroll bound once.
    app.file_scroll_right();
    // Rename to a longer path while keeping the file count constant.
    // Length-keyed invalidation alone would miss this; the cache must
    // clear on every `set_files` assignment.
    tx.send(SnapshotMsg::Ok(
        RepoSnapshot {
            files: vec![ChangedFile::unstaged_only(
                "a_much_longer_renamed_path.rs".to_string(),
                StatusKind::Modified,
            )],
            tracking: None,
            head_oid: None,
            branch_name: None,
            refs_fingerprint: 0,
        },
        HashMap::new(),
    ))
    .unwrap();
    app.poll_snapshot();
    // Drive enough right-scrolls to reach the new max; if the cache were
    // stale we would clamp at the old (shorter) bound.
    for _ in 0..20 {
        app.file_scroll_right();
    }
    assert!(app.status_view.file_scroll_x >= "short.rs".chars().count());
}

#[test]
fn snapshot_refresh_with_no_filter_matches_clears_stale_diff() {
    let (snapshot, tx) = dummy_snapshot_channel();
    let mut app = App {
        snapshot,
        pending_snapshot: None,
        ..app_with_files(vec!["bar.rs"])
    };
    app.status_view.search_query.set("bar");
    app.status_view.recompute_filter();
    app.diff.set_hunks(vec![context_hunk(&["stale"])]);

    tx.send(SnapshotMsg::Ok(
        RepoSnapshot {
            files: vec![ChangedFile::unstaged_only(
                "aaa.rs".to_string(),
                StatusKind::Modified,
            )],
            tracking: None,
            head_oid: None,
            branch_name: None,
            refs_fingerprint: 0,
        },
        HashMap::new(),
    ))
    .unwrap();
    app.poll_snapshot();

    assert!(app.filtered_indices().is_empty());
    assert!(app.diff.hunks().is_empty());
}

#[test]
fn non_selected_file_change_does_not_reload_the_selected_diff() {
    let mut app = app_with_files(vec!["selected.rs", "other.rs"]);
    app.repo_path = "missing-repo-used-to-detect-unwanted-load".to_string();
    app.diff.hunks = vec![context_hunk(&["selected diff"])];
    let selected_mtime = SystemTime::UNIX_EPOCH + Duration::from_secs(10);
    app.status_view
        .hot_table
        .insert("selected.rs".to_string(), selected_mtime);

    app.ingest_snapshot(
        RepoSnapshot {
            files: vec![
                ChangedFile::unstaged_only("selected.rs".to_string(), StatusKind::Modified),
                ChangedFile::unstaged_only("other.rs".to_string(), StatusKind::Modified),
            ],
            tracking: None,
            head_oid: None,
            branch_name: None,
            refs_fingerprint: 0,
        },
        HashMap::from([
            ("selected.rs".to_string(), selected_mtime),
            (
                "other.rs".to_string(),
                SystemTime::UNIX_EPOCH + Duration::from_secs(20),
            ),
        ]),
    );

    assert_eq!(app.diff.hunks[0].lines[0].content, "selected diff");
    assert!(
        app.notice
            .as_ref()
            .is_none_or(|notice| notice.kind != NoticeKind::Diff),
        "an unchanged selection must not start a repository load"
    );
}
