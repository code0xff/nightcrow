use super::*;

/// A temp repo with `src/main.rs` and `README.md` at the root, plus an app
/// pointed at it. The app uses an inert snapshot channel (no worker).
pub(crate) fn make_tree_repo() -> (tempfile::TempDir, String) {
    let (dir, path) = make_repo();
    let root = Path::new(&path);
    std::fs::create_dir(root.join("src")).unwrap();
    std::fs::write(root.join("src").join("main.rs"), "fn main() {}\n").unwrap();
    std::fs::write(root.join("README.md"), "# hi\n").unwrap();
    (dir, path)
}

pub(crate) fn app_on(path: &str) -> App {
    let mut app = app_with_files(vec![]);
    app.repo_path = path.to_string();
    app
}

pub(crate) fn tree_index_of(app: &App, path: &str) -> usize {
    app.tree_view
        .visible_rows()
        .iter()
        .position(|r| r.path == path)
        .unwrap_or_else(|| panic!("{path} not visible in tree"))
}

#[test]
fn enter_tree_mode_loads_root_and_shows_file_overlay() {
    let (dir, path) = make_tree_repo();
    let mut app = app_on(&path);

    app.enter_tree_mode();

    assert_eq!(app.mode, ViewMode::Tree);
    let rows = app.tree_view.visible_rows();
    // Directories sort first: src/ before README.md.
    assert_eq!(rows[0].path, "src");
    assert!(rows[0].is_dir);
    assert!(rows.iter().any(|r| r.path == "README.md"));
    // The right pane is always the file overlay in Tree mode.
    assert_eq!(app.diff.view, DiffPaneView::File);
    drop(dir);
}

#[test]
fn tree_expand_reveals_children_and_collapse_hides_them() {
    let (dir, path) = make_tree_repo();
    let mut app = app_on(&path);
    app.enter_tree_mode();

    app.tree_view.selected = tree_index_of(&app, "src");
    app.tree_expand();
    assert!(
        app.tree_view
            .visible_rows()
            .iter()
            .any(|r| r.path == "src/main.rs"),
        "expanding src must reveal its child"
    );

    // Cursor is back on the (still-selected) src row; collapsing hides it.
    app.tree_view.selected = tree_index_of(&app, "src");
    app.tree_collapse();
    assert!(
        !app.tree_view
            .visible_rows()
            .iter()
            .any(|r| r.path == "src/main.rs"),
        "collapsing src must hide its child"
    );
    drop(dir);
}

#[test]
fn selecting_tree_file_loads_raw_contents_into_file_view() {
    let (dir, path) = make_tree_repo();
    let mut app = app_on(&path);
    app.enter_tree_mode();

    app.tree_view.selected = tree_index_of(&app, "README.md");
    app.preview_tree_selected();
    app.flush_git_loads_for_test(Duration::from_secs(2));

    assert_eq!(app.diff.view, DiffPaneView::File);
    assert_eq!(
        app.diff.file_view.key,
        Some(FileViewKey::Status("README.md".to_string()))
    );
    assert_eq!(app.diff.file_view.content, "# hi\n");
    drop(dir);
}

#[test]
fn tree_collapse_on_expanded_child_steps_to_parent() {
    let (dir, path) = make_tree_repo();
    let mut app = app_on(&path);
    app.enter_tree_mode();
    app.tree_view.selected = tree_index_of(&app, "src");
    app.tree_expand();

    // Sit on the child file, then collapse: the cursor walks up to `src`.
    app.tree_view.selected = tree_index_of(&app, "src/main.rs");
    app.tree_collapse();

    assert_eq!(
        app.tree_view.selected_path().as_deref(),
        Some("src"),
        "Left on a child should move selection to its parent dir"
    );
    drop(dir);
}

#[test]
fn tree_search_finds_file_in_unexpanded_dir() {
    let (dir, path) = make_tree_repo();
    let mut app = app_on(&path);
    app.enter_tree_mode();
    // `src` starts collapsed, so its child is not in the normal view.
    assert!(!app.tree_view.expanded.contains("src"));

    app.start_tree_search();
    for ch in "main".chars() {
        app.tree_search_push(ch);
    }

    // The match is revealed through its ancestor chain even though `src`
    // was never manually expanded.
    let rows = app.tree_view.visible_rows();
    assert!(rows.iter().any(|r| r.path == "src/main.rs"));
    assert!(rows.iter().any(|r| r.path == "src"));
    assert!(!rows.iter().any(|r| r.path == "README.md"));
    // Cursor lands on the matching file, not the connecting `src` dir.
    assert_eq!(
        app.tree_view.selected_path().as_deref(),
        Some("src/main.rs")
    );
    // Filtering must not mutate the real expansion set.
    assert!(!app.tree_view.expanded.contains("src"));
    drop(dir);
}

#[test]
fn confirm_tree_search_reveals_match_in_normal_view() {
    let (dir, path) = make_tree_repo();
    let mut app = app_on(&path);
    app.enter_tree_mode();

    app.start_tree_search();
    for ch in "main".chars() {
        app.tree_search_push(ch);
    }
    app.confirm_tree_search();

    // Overlay closed, query cleared, and `src` is now genuinely expanded so
    // the chosen file stays visible with the cursor on it.
    assert!(!app.tree_view.search_active);
    assert!(app.tree_view.search_query.is_empty());
    assert!(app.tree_view.expanded.contains("src"));
    assert_eq!(
        app.tree_view.selected_path().as_deref(),
        Some("src/main.rs")
    );
    drop(dir);
}

#[test]
fn cancel_tree_search_leaves_expansion_untouched() {
    let (dir, path) = make_tree_repo();
    let mut app = app_on(&path);
    app.enter_tree_mode();

    app.start_tree_search();
    for ch in "main".chars() {
        app.tree_search_push(ch);
    }
    app.cancel_tree_search();

    assert!(!app.tree_view.search_active);
    assert!(app.tree_view.search_query.is_empty());
    // Esc must not expand anything; the view returns to its prior state.
    assert!(!app.tree_view.expanded.contains("src"));
    assert!(
        !app.tree_view
            .visible_rows()
            .iter()
            .any(|r| r.path == "src/main.rs")
    );
    drop(dir);
}

#[test]
fn toggle_tree_mode_round_trips_status_and_tree() {
    let (dir, path) = make_tree_repo();
    let mut app = app_on(&path);
    assert_eq!(app.mode, ViewMode::Status);

    app.toggle_tree_mode();
    assert_eq!(app.mode, ViewMode::Tree);

    app.toggle_tree_mode();
    assert_eq!(app.mode, ViewMode::Status);
    drop(dir);
}

#[test]
fn enter_tree_mode_picks_up_dir_created_after_first_entry() {
    let (dir, path) = make_tree_repo();
    let mut app = app_on(&path);
    // First entry caches the root listing (no `docs/` yet).
    app.enter_tree_mode();
    assert!(
        !app.tree_view
            .visible_rows()
            .iter()
            .any(|r| r.path == "docs"),
        "docs should not exist before it is created"
    );

    // Create a directory on disk while away from Tree mode.
    app.exit_tree_to_status();
    std::fs::create_dir(Path::new(&path).join("docs")).unwrap();
    std::fs::write(Path::new(&path).join("docs").join("guide.md"), "x").unwrap();

    // Re-entering must re-read the root and surface the new directory.
    app.enter_tree_mode();
    assert!(
        app.tree_view
            .visible_rows()
            .iter()
            .any(|r| r.path == "docs"),
        "re-entering Tree mode must reflect the newly created directory"
    );
    drop(dir);
}

#[test]
fn enter_tree_mode_reflects_moved_dir_without_error() {
    let (dir, path) = make_tree_repo();
    let mut app = app_on(&path);
    // Cache the root with `src/` present, expanded.
    app.enter_tree_mode();
    app.tree_view.selected = tree_index_of(&app, "src");
    app.tree_expand();
    assert!(app.tree_view.expanded.contains("src"));

    // Move `src/` to `lib/` on disk while away from Tree mode.
    app.exit_tree_to_status();
    std::fs::rename(Path::new(&path).join("src"), Path::new(&path).join("lib")).unwrap();

    app.enter_tree_mode();
    let rows = app.tree_view.visible_rows();
    assert!(
        !rows.iter().any(|r| r.path == "src"),
        "the moved-away directory must disappear from its old location"
    );
    assert!(
        rows.iter().any(|r| r.path == "lib"),
        "the directory must appear at its new location"
    );
    // The stale `src` expansion is pruned (it no longer exists), so no
    // failing re-read leaks a "tree error" into the status bar.
    assert!(!app.tree_view.expanded.contains("src"));
    assert!(
        !app.notice
            .as_ref()
            .is_some_and(|n| n.kind == NoticeKind::Tree),
        "a vanished directory must not surface a tree error: {:?}",
        app.notice
    );
    drop(dir);
}
