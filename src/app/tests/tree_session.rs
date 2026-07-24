use super::*;
use super::tree::{app_on, make_tree_repo, tree_index_of};

#[test]
fn a_change_in_a_collapsed_directory_updates_search_results() {
    // The filtered view spans the whole tree, so a file created under a
    // directory the user never expanded still changes the results. The
    // watch set follows the index while a search is open, which is what
    // makes the event arrive at all.
    use crate::runtime::tree_watch::TreeWatcher;
    let (dir, path) = make_tree_repo();
    let mut app = app_on(&path);
    let (tx, rx) = std::sync::mpsc::channel();
    app.tree_watch = TreeWatcher::from_receiver(rx);
    app.enter_tree_mode();
    app.start_tree_search();
    for c in "main".chars() {
        app.tree_search_push(c);
    }
    let before = app.tree_view.match_count;
    assert!(
        !app.tree_view.expanded.contains("src"),
        "src stays collapsed — the point of the test"
    );

    std::fs::write(Path::new(&path).join("src").join("main_two.rs"), "\n").unwrap();
    tx.send(Ok(vec![notify_debouncer_mini::DebouncedEvent {
        path: Path::new(&path).join("src").join("main_two.rs"),
        kind: notify_debouncer_mini::DebouncedEventKind::Any,
    }]))
    .unwrap();
    app.poll_tree_watcher();

    assert_eq!(app.tree_view.match_count, before + 1);
    drop(dir);
}

#[test]
fn a_watcher_refresh_updates_active_search_results() {
    // The filtered view renders from the search index, so refreshing only
    // the directory cache left a new file out of the results and the match
    // count stale until the query changed.
    let (dir, path) = make_tree_repo();
    let mut app = app_on(&path);
    app.enter_tree_mode();
    app.start_tree_search();
    for c in "main".chars() {
        app.tree_search_push(c);
    }
    let before = app.tree_view.match_count;

    std::fs::write(Path::new(&path).join("src").join("main_two.rs"), "\n").unwrap();
    app.refresh_tree_preserving_cursor();

    assert_eq!(
        app.tree_view.match_count,
        before + 1,
        "a file created while the search is open must join the results"
    );
    drop(dir);
}

#[test]
fn a_hidden_tree_change_is_remembered_until_the_tab_is_shown() {
    // Rereading directories is the expensive half; a hidden project only
    // records that it must, so filesystem churn elsewhere cannot stall the
    // active tab.
    let mut app = app_with_files(vec!["a.rs"]);
    app.mode = ViewMode::Tree;
    app.tree_dirty.insert("src".to_string());

    // Draining with no new event leaves the flag standing, so the refresh
    // still happens once this project becomes the active one.
    app.drain_tree_watcher();
    assert!(
        !app.tree_dirty.is_empty(),
        "a pending refresh survives a drain"
    );

    app.poll_tree_watcher();
    assert!(app.tree_dirty.is_empty(), "the active project consumes it");
}

#[test]
fn tree_preview_survives_status_snapshot() {
    let (dir, path) = make_tree_repo();
    let (snapshot, tx) = dummy_snapshot_channel();
    let mut app = App {
        snapshot,
        pending_snapshot: None,
        ..app_on(&path)
    };
    app.enter_tree_mode();
    app.tree_view.selected = tree_index_of(&app, "README.md");
    app.preview_tree_selected();
    let content_before = app.diff.file_view.content.clone();

    // A git-status snapshot arrives (e.g. file changed in a terminal pane).
    tx.send(SnapshotMsg::Ok(
        RepoSnapshot {
            files: Vec::new(),
            tracking: None,
            head_oid: None,
            branch_name: None,
        },
        HashMap::new(),
    ))
    .unwrap();
    app.poll_snapshot();

    // Tree mode and its preview must be untouched by the snapshot ingest.
    assert_eq!(app.mode, ViewMode::Tree);
    assert_eq!(app.diff.view, DiffPaneView::File);
    assert_eq!(app.diff.file_view.content, content_before);
    drop(dir);
}

#[test]
fn restoring_tree_session_clears_lingering_status_search() {
    // Reachable case: `/` opens status search before the first snapshot,
    // then a pending session restores Tree mode. The stale search overlay
    // must be cleared so Tree keystrokes aren't captured by the search
    // handler.
    let (dir, path) = make_tree_repo();
    let mut app = app_on(&path);
    app.start_search();
    app.search_push('x');
    assert!(app.status_view.search_active);

    app.restore_session(&crate::session::SessionState {
        mode: Some(ViewMode::Tree),
        ..Default::default()
    });

    assert_eq!(app.mode, ViewMode::Tree);
    assert!(
        !app.status_view.search_active,
        "restoring Tree mode must clear a lingering status search overlay"
    );
    assert!(app.status_view.search_query.is_empty());
    drop(dir);
}

#[test]
fn entering_tree_mode_clears_lingering_status_search() {
    let (dir, path) = make_tree_repo();
    let mut app = app_on(&path);
    app.start_search();
    app.search_push('x');
    assert!(app.status_view.search_active);

    app.enter_tree_mode();

    assert!(!app.status_view.search_active);
    assert!(app.status_view.search_query.is_empty());
    drop(dir);
}

#[test]
fn restore_tree_session_ignores_unsafe_expanded_paths() {
    // A corrupted/hand-edited session must not be able to drive the tree
    // to read directories outside the working tree.
    let (dir, path) = make_tree_repo();
    let mut app = app_on(&path);

    app.restore_session(&crate::session::SessionState {
        mode: Some(ViewMode::Tree),
        tree_expanded: vec![
            "../../..".to_string(),
            "/etc".to_string(),
            "src".to_string(),
        ],
        ..Default::default()
    });

    assert_eq!(app.mode, ViewMode::Tree);
    // Only the safe, real directory was expanded/cached.
    assert!(app.tree_view.expanded.contains("src"));
    assert!(!app.tree_view.expanded.contains("../../.."));
    assert!(!app.tree_view.expanded.contains("/etc"));
    assert!(!app.tree_view.cache.contains_key("/etc"));
    drop(dir);
}

#[test]
fn restore_tree_session_prunes_expansion_gone_since_save() {
    // A directory that was expanded when the session was saved may have been
    // moved/deleted before the next launch. Restore must drop it rather than
    // re-reading a now-missing path and leaking a "tree error".
    let (dir, path) = make_tree_repo();
    std::fs::rename(Path::new(&path).join("src"), Path::new(&path).join("lib")).unwrap();
    let mut app = app_on(&path);

    app.restore_session(&crate::session::SessionState {
        mode: Some(ViewMode::Tree),
        tree_expanded: vec!["src".to_string()],
        tree_selected_path: Some("src".to_string()),
        ..Default::default()
    });

    assert_eq!(app.mode, ViewMode::Tree);
    // `src` no longer exists on disk, so it must not be kept as expanded...
    assert!(!app.tree_view.expanded.contains("src"));
    // ...and the moved-to directory is visible at the root.
    assert!(app.tree_view.visible_rows().iter().any(|r| r.path == "lib"));
    assert!(
        !app.notice
            .as_ref()
            .is_some_and(|n| n.kind == NoticeKind::Tree),
        "a vanished restored expansion must not surface a tree error: {:?}",
        app.notice
    );
    drop(dir);
}

#[test]
fn entering_tree_cancels_in_flight_commit_log_fetch() {
    // A page fetch spawned in Log mode must be torn down on Tree entry so
    // its reply can't load a commit diff over the Tree file preview.
    let (dir, path) = make_tree_repo();
    let mut app = app_on(&path);
    app.spawn_commit_log_refresh_fetch(None, None);
    assert!(app.pagination.page_rx.is_some(), "fetch should be pending");

    app.enter_tree_mode();

    assert!(
        app.pagination.page_rx.is_none(),
        "entering Tree mode must cancel the in-flight commit-log fetch"
    );
    drop(dir);
}

#[test]
fn tree_mode_diff_file_and_split_toggles_are_noops() {
    let (dir, path) = make_tree_repo();
    let mut app = app_on(&path);
    app.enter_tree_mode();
    assert_eq!(app.diff.view, DiffPaneView::File);

    // `v` and `s` must not flip the right pane away from the file preview.
    app.toggle_diff_file_view();
    assert_eq!(app.diff.view, DiffPaneView::File);
    app.toggle_diff_split_view();
    assert_eq!(app.diff.view, DiffPaneView::File);
    drop(dir);
}

#[test]
fn tree_session_round_trips_mode_expansion_and_selection() {
    let (dir, path) = make_tree_repo();
    let mut app = app_on(&path);
    app.enter_tree_mode();
    app.tree_view.selected = tree_index_of(&app, "src");
    app.tree_expand();
    app.tree_view.selected = tree_index_of(&app, "src/main.rs");

    let state = app.save_session();
    assert_eq!(state.mode, Some(ViewMode::Tree));
    assert!(state.tree_expanded.contains(&"src".to_string()));
    assert_eq!(state.tree_selected_path.as_deref(), Some("src/main.rs"));

    let mut other = app_on(&path);
    other.restore_session(&state);
    assert_eq!(other.mode, ViewMode::Tree);
    assert!(other.tree_view.expanded.contains("src"));
    assert_eq!(
        other.tree_view.selected_path().as_deref(),
        Some("src/main.rs")
    );
    // The restored selection previews the file, not a diff.
    assert_eq!(other.diff.view, DiffPaneView::File);
    assert_eq!(other.diff.file_view.content, "fn main() {}\n");
    drop(dir);
}