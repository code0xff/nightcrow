use super::*;

#[test]
fn selection_clamps_when_file_list_shrinks() {
    let mut app = app_with_files(vec!["a.rs", "b.rs", "c.rs"]);
    app.git.view.status.selected = 2;
    app.git.view.status.files = vec![ChangedFile::unstaged_only(
        "a.rs".to_string(),
        StatusKind::Modified,
    )];

    let selected_path = app.restore_selection(Some("c.rs"));

    assert_eq!(selected_path.as_deref(), Some("a.rs"));
    assert_eq!(app.git.view.status.selected, 0);
}

#[test]
fn selection_prefers_same_path_after_refresh() {
    let mut app = app_with_files(vec!["a.rs", "b.rs", "c.rs"]);
    app.git.view.status.selected = 1;
    app.git.view.status.files = vec![
        ChangedFile::unstaged_only("a.rs".to_string(), StatusKind::Modified),
        ChangedFile::unstaged_only("c.rs".to_string(), StatusKind::Modified),
        ChangedFile::unstaged_only("b.rs".to_string(), StatusKind::Modified),
    ];

    let selected_path = app.restore_selection(Some("b.rs"));

    assert_eq!(selected_path.as_deref(), Some("b.rs"));
    assert_eq!(app.git.view.status.selected, 2);
}

#[test]
fn diff_scroll_saturates_on_page_up() {
    let mut app = app_with_files(vec!["a.rs"]);
    app.focus = Focus::DiffViewer;
    app.git.view.diff.scroll = 3;

    app.page_up();

    assert_eq!(app.git.view.diff.scroll, 0);
}

#[test]
fn diff_scroll_clamps_at_last_line_on_select_down() {
    let mut app = app_with_files(vec!["a.rs"]);
    app.focus = Focus::DiffViewer;
    // 1 hunk = header + 1 content line = 2 total lines, max_scroll = 1
    app.git.view.diff.set_hunks(vec![context_hunk(&["x"])]);
    app.git.view.diff.scroll = 1; // already at max

    app.select_down();

    assert_eq!(
        app.git.view.diff.scroll, 1,
        "scroll must not exceed last line index"
    );
}

#[test]
fn diff_scroll_clamps_at_last_line_on_page_down() {
    let mut app = app_with_files(vec!["a.rs"]);
    app.focus = Focus::DiffViewer;
    app.git.view.diff.set_hunks(vec![context_hunk(&["x"])]);
    app.git.view.diff.scroll = 0;

    app.page_down(); // +20, but max is 1

    assert_eq!(app.git.view.diff.scroll, 1);
}

#[test]
fn diff_scroll_handles_large_restored_offset() {
    let mut app = app_with_files(vec!["a.rs"]);
    app.focus = Focus::DiffViewer;
    app.git.view.diff.set_hunks(vec![context_hunk(&["x"])]);
    app.git.view.diff.scroll = usize::MAX;

    app.select_down();

    assert_eq!(app.git.view.diff.scroll, 1);
}

#[test]
fn diff_match_refresh_can_preserve_manual_scroll() {
    let mut app = app_with_files(vec!["a.rs"]);
    app.git.view.diff.set_hunks(vec![context_hunk(&["needle"])]);
    app.git.view.diff.search.query.set("needle");
    app.git.view.diff.scroll = 7;

    app.git.view.diff.recompute_matches(false);

    assert_eq!(app.git.view.diff.search.matches, vec![1]);
    assert_eq!(app.git.view.diff.scroll, 7);
}

#[test]
fn diff_search_input_scrolls_to_first_match() {
    let mut app = app_with_files(vec!["a.rs"]);
    app.git
        .view
        .diff
        .set_hunks(vec![context_hunk(&["alpha", "needle"])]);

    app.git.view.diff.search_push('n');

    assert_eq!(app.git.view.diff.search.matches, vec![2]);
    assert_eq!(app.git.view.diff.scroll, 2);
}

#[test]
fn status_search_with_no_matches_clears_stale_diff() {
    let mut app = app_with_files(vec!["a.rs"]);
    app.git.view.diff.set_hunks(vec![context_hunk(&["stale"])]);

    app.search_push('z');

    assert!(app.filtered_indices().is_empty());
    assert!(app.git.view.diff.hunks().is_empty());
}
