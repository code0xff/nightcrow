use super::*;

#[test]
fn keep_scroll_clamps_when_new_diff_is_shorter() {
    let mut app = app_with_files(vec!["a.rs"]);
    // Seed a long diff and put scroll near the bottom.
    app.git.view.diff.set_hunks(vec![
        context_hunk(&["l1", "l2", "l3", "l4", "l5"]),
        context_hunk(&["l6", "l7", "l8"]),
    ]);
    app.git.view.diff.scroll = app.git.view.diff.max_scroll();
    let prev_scroll = app.git.view.diff.scroll;
    assert!(prev_scroll > 1);

    // Apply a much shorter diff with KeepScroll; scroll must clamp.
    let shorter = vec![context_hunk(&["only"])];
    app.apply_diff_result(Ok(shorter), DiffApply::KeepScroll(prev_scroll));
    assert!(
        app.git.view.diff.scroll <= app.git.view.diff.max_scroll(),
        "scroll {} exceeded max {}",
        app.git.view.diff.scroll,
        app.git.view.diff.max_scroll()
    );
}

#[test]
fn toggle_diff_file_view_ignores_selection_outside_filter() {
    let mut app = app_with_files(vec!["alpha.rs", "bravo.rs"]);
    app.git.view.status.search_query.set("alpha");
    app.git.view.status.recompute_filter();
    // selected points outside the filter — toggle must refuse to open
    // a file view rather than loading the hidden entry.
    app.git.view.status.selected = 1;
    app.toggle_diff_file_view();
    assert_eq!(app.git.view.diff.view, DiffPaneView::Diff);
    assert!(app.git.view.diff.file_view.key.is_none());
}

#[test]
fn cycling_the_diff_view_walks_all_three_and_returns_to_the_start() {
    let mut app = app_with_files(vec!["a.rs"]);
    assert_eq!(app.git.view.diff.view, DiffPaneView::Diff);

    app.cycle_diff_view();
    assert_eq!(app.git.view.diff.view, DiffPaneView::Split);
    app.cycle_diff_view();
    assert_eq!(app.git.view.diff.view, DiffPaneView::File);
    app.cycle_diff_view();
    assert_eq!(
        app.git.view.diff.view,
        DiffPaneView::Diff,
        "the cycle must close so one key can reach every view"
    );
}

#[test]
fn cycling_skips_the_file_view_when_there_is_nothing_to_open() {
    // Selection outside the filter leaves no resolvable file, the same gate
    // that makes `v` a no-op. Skipping keeps the press from doing nothing.
    let mut app = app_with_files(vec!["alpha.rs", "bravo.rs"]);
    app.git.view.status.search_query.set("alpha");
    app.git.view.status.recompute_filter();
    app.git.view.status.selected = 1;
    assert!(!app.can_open_file_view());

    app.cycle_diff_view();
    assert_eq!(app.git.view.diff.view, DiffPaneView::Split);
    app.cycle_diff_view();
    assert_eq!(
        app.git.view.diff.view,
        DiffPaneView::Diff,
        "with no file to show the cycle is unified <-> split"
    );
}

#[test]
fn cycling_the_diff_view_does_nothing_in_tree_mode() {
    // Tree mode's right pane is always the raw file preview, so there is no
    // cycle to walk — matching `v`/`s`.
    let mut app = app_with_files(vec!["a.rs"]);
    app.git.view.mode = ViewMode::Tree;
    app.git.view.diff.view = DiffPaneView::File;

    app.cycle_diff_view();

    assert_eq!(app.git.view.diff.view, DiffPaneView::File);
}

#[test]
fn toggle_diff_split_view_round_trips_and_overrides_file_view() {
    let mut app = app_with_files(vec!["a.rs"]);

    // Diff → Split → Diff.
    app.toggle_diff_split_view();
    assert_eq!(app.git.view.diff.view, DiffPaneView::Split);
    app.toggle_diff_split_view();
    assert_eq!(app.git.view.diff.view, DiffPaneView::Diff);

    // From the file overlay, the split toggle switches straight to Split
    // rather than back to the unified diff.
    app.git.view.diff.view = DiffPaneView::File;
    app.toggle_diff_split_view();
    assert_eq!(app.git.view.diff.view, DiffPaneView::Split);
}

#[test]
fn keep_scroll_preserves_open_file_view() {
    let mut app = app_with_files(vec!["a.rs"]);
    app.git
        .view
        .diff
        .set_hunks(vec![context_hunk(&["l1", "l2"])]);
    app.git.view.diff.scroll = 1;
    app.git.view.diff.file_view = seeded_file_view("a.rs");
    app.git.view.diff.view = DiffPaneView::File;

    // Same file refresh through KeepScroll must leave the file view
    // alone — only Reset paths should invalidate it.
    let fresh = vec![context_hunk(&["l1", "l2", "l3"])];
    app.apply_diff_result(Ok(fresh), DiffApply::KeepScroll(app.git.view.diff.scroll));

    assert_eq!(app.git.view.diff.view, DiffPaneView::File);
    assert_eq!(
        app.git.view.diff.file_view.key,
        Some(FileViewKey::Status("a.rs".into()))
    );
    assert_eq!(app.git.view.diff.file_view.scroll, 1);
    assert_eq!(app.git.view.diff.file_view.scroll_x, 4);
}

#[test]
fn clear_diff_state_invalidates_open_file_view() {
    let mut app = app_with_files(vec!["a.rs"]);
    app.git.view.diff.set_hunks(vec![context_hunk(&["l1"])]);
    app.git.view.diff.file_view = seeded_file_view("a.rs");
    app.git.view.diff.view = DiffPaneView::File;

    // toggle_mode and other reset paths route through clear_diff_state
    // — that single call must wipe the file view to its default.
    app.clear_diff_state();

    assert_eq!(app.git.view.diff.view, DiffPaneView::Diff);
    assert!(app.git.view.diff.file_view.key.is_none());
    assert!(app.git.view.diff.file_view.content.is_empty());
    assert_eq!(app.git.view.diff.file_view.scroll, 0);
    assert_eq!(app.git.view.diff.file_view.scroll_x, 0);
}

#[test]
fn snapshot_refresh_with_no_filter_matches_clears_file_view() {
    let (snapshot, tx) = dummy_snapshot_channel();
    let mut app = app_with_files(vec!["bar.rs"]);
    app.git.snapshot = snapshot;
    app.git.view.status.search_query.set("bar");
    app.git.view.status.recompute_filter();
    app.git.view.diff.set_hunks(vec![context_hunk(&["stale"])]);
    app.git.view.diff.file_view = seeded_file_view("bar.rs");
    app.git.view.diff.view = DiffPaneView::File;

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

    // No filter matches the new snapshot, so the diff and file view
    // both need to drop their stale handles on the gone path.
    assert!(app.filtered_indices().is_empty());
    assert!(app.git.view.diff.hunks().is_empty());
    assert_eq!(app.git.view.diff.view, DiffPaneView::Diff);
    assert!(app.git.view.diff.file_view.key.is_none());
}

#[test]
fn enabling_wrap_drops_the_stale_horizontal_offset() {
    // ratatui ignores scroll.x while wrapping, so an offset left behind would
    // reappear the moment wrap is switched back off.
    let mut app = app_with_files(vec!["a.rs"]);
    app.git.view.diff.scroll_x = 12;
    app.git.view.diff.file_view.scroll_x = 9;

    app.toggle_diff_wrap();

    assert!(app.git.view.diff.wrap);
    assert_eq!(app.git.view.diff.scroll_x, 0);
    assert_eq!(app.git.view.diff.file_view.scroll_x, 0);
}

#[test]
fn disabling_wrap_leaves_the_offset_where_it_was_reset() {
    let mut app = app_with_files(vec!["a.rs"]);
    app.toggle_diff_wrap();
    app.toggle_diff_wrap();

    assert!(!app.git.view.diff.wrap);
    assert_eq!(
        app.git.view.diff.scroll_x, 0,
        "turning wrap off must not resurrect a pre-wrap offset"
    );
}
