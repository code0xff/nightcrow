use super::*;

#[test]
fn move_selected_in_filter_resets_horizontal_scroll() {
    let mut app = app_with_files(vec!["a.rs", "b.rs"]);
    app.git.view.status.file_scroll_x = 12;
    app.move_selected_in_filter(1);
    assert_eq!(app.git.view.status.selected, 1);
    assert_eq!(app.git.view.status.file_scroll_x, 0);
}

#[test]
fn log_select_down_resets_commit_scroll() {
    let mut app = app_with_files(vec![]);
    app.git.view.mode = ViewMode::Log;
    // Seed through `set_commits` so the search filter cache is built;
    // log navigation walks the filter cache (empty query → 0..len),
    // which matches the production code path.
    app.git.view.log.set_commits(vec![
        CommitEntry::new(
            git2::Oid::ZERO_SHA1,
            "0000000".into(),
            "first".into(),
            "T".into(),
            0,
        ),
        CommitEntry::new(
            git2::Oid::ZERO_SHA1,
            "1111111".into(),
            "second".into(),
            "T".into(),
            0,
        ),
    ]);
    app.git.view.log.commit_scroll_x = 9;
    app.log_select_down();
    assert_eq!(app.git.view.log.selected, 1);
    assert_eq!(app.git.view.log.commit_scroll_x, 0);
}

#[test]
fn log_file_select_down_resets_file_scroll() {
    let mut app = app_with_files(vec![]);
    app.git.view.mode = ViewMode::Log;
    app.git.view.log.drill_down = true;
    app.git.view.log.set_commits(vec![CommitEntry::new(
        git2::Oid::ZERO_SHA1,
        "0000000".into(),
        "first".into(),
        "T".into(),
        0,
    )]);
    app.git.view.log.set_commit_files(vec![
        ChangedFile::unstaged_only("x.rs".into(), StatusKind::Modified),
        ChangedFile::unstaged_only("y.rs".into(), StatusKind::Modified),
    ]);
    app.git.view.log.file_scroll_x = 7;
    app.log_file_select_down();
    assert_eq!(app.git.view.log.file_selected, 1);
    assert_eq!(app.git.view.log.file_scroll_x, 0);
}

#[test]
fn diff_scroll_routes_to_file_view_when_in_file_mode() {
    let mut app = app_with_files(vec![]);
    app.git.view.diff.scroll_x = 12;
    app.git.view.diff.file_view.scroll_x = 4;
    app.git.view.diff.view = DiffPaneView::File;

    app.git.view.diff.scroll_right();
    assert_eq!(
        app.git.view.diff.scroll_x, 12,
        "diff scroll_x must not change"
    );
    assert_eq!(app.git.view.diff.file_view.scroll_x, 8);

    app.git.view.diff.scroll_left();
    assert_eq!(app.git.view.diff.file_view.scroll_x, 4);

    app.git.view.diff.view = DiffPaneView::Diff;
    app.git.view.diff.scroll_right();
    assert_eq!(app.git.view.diff.scroll_x, 16);
    assert_eq!(
        app.git.view.diff.file_view.scroll_x, 4,
        "file_view scroll_x must not change in diff mode"
    );
}

#[test]
fn selected_filtered_status_file_returns_none_outside_filter() {
    let mut app = app_with_files(vec!["alpha.rs", "bravo.rs", "charlie.rs"]);
    app.git.view.status.search_query.set("alpha");
    app.git.view.status.recompute_filter();
    // Filter only matches index 0; selecting index 2 must return None.
    app.git.view.status.selected = 2;
    assert!(app.selected_filtered_status_file().is_none());

    app.git.view.status.selected = 0;
    assert_eq!(
        app.selected_filtered_status_file().map(|f| f.path.as_str()),
        Some("alpha.rs")
    );
}
