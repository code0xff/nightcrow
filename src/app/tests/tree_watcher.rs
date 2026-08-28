use super::tree::{app_on, make_tree_repo, tree_index_of};
use super::*;

#[test]
fn refresh_tree_cache_keeps_expansion_for_surviving_dirs() {
    let (dir, path) = make_tree_repo();
    let mut app = app_on(&path);
    app.enter_tree_mode();
    app.git.view.tree.selected = tree_index_of(&app, "src");
    app.tree_expand();
    assert!(
        app.git
            .view
            .tree
            .visible_rows()
            .iter()
            .any(|r| r.path == "src/main.rs"),
        "src should be expanded before the refresh"
    );

    app.refresh_tree_cache();

    assert!(app.git.view.tree.expanded.contains("src"));
    assert!(
        app.git
            .view
            .tree
            .visible_rows()
            .iter()
            .any(|r| r.path == "src/main.rs"),
        "a surviving directory keeps its expansion and re-read children"
    );
    drop(dir);
}

#[test]
fn enter_tree_mode_keeps_cursor_on_same_path_when_rows_shift() {
    let (dir, path) = make_tree_repo();
    let mut app = app_on(&path);
    app.enter_tree_mode();
    // Park the cursor on README.md.
    app.git.view.tree.selected = tree_index_of(&app, "README.md");

    // Insert a directory that sorts ahead of everything, shifting README.md
    // down by one row.
    app.exit_tree_to_status();
    std::fs::create_dir(Path::new(&path).join("aaa")).unwrap();

    app.enter_tree_mode();
    // The cursor must follow README.md, not stay on its old index (which now
    // points at a different row).
    assert_eq!(
        app.git.view.tree.selected_path().as_deref(),
        Some("README.md"),
        "cursor must track its path across the row-set shift"
    );
    drop(dir);
}

#[test]
fn poll_tree_watcher_refreshes_tree_on_event_in_tree_mode() {
    use crate::runtime::tree_watch::TreeWatcher;
    let (dir, path) = make_tree_repo();
    let mut app = app_on(&path);
    // Swap in a watcher we can feed synthetic events into.
    let (tx, rx) = std::sync::mpsc::channel();
    app.git.view.tree_watch = TreeWatcher::from_receiver(rx);
    app.enter_tree_mode();
    assert!(
        !app.git
            .view
            .tree
            .visible_rows()
            .iter()
            .any(|r| r.path == "docs")
    );

    // A folder appears on disk, then the watcher fires with its path.
    std::fs::create_dir(Path::new(&path).join("docs")).unwrap();
    tx.send(Ok(vec![notify_debouncer_mini::DebouncedEvent {
        path: Path::new(&path).join("docs"),
        kind: notify_debouncer_mini::DebouncedEventKind::Any,
    }]))
    .unwrap();
    app.poll_tree_watcher();

    assert!(
        app.git
            .view
            .tree
            .visible_rows()
            .iter()
            .any(|r| r.path == "docs"),
        "a watcher event in Tree mode must re-read and surface the new dir"
    );
    drop(dir);
}

#[test]
fn poll_tree_watcher_ignores_events_outside_tree_mode() {
    use crate::runtime::tree_watch::TreeWatcher;
    let (dir, path) = make_tree_repo();
    let mut app = app_on(&path);
    let (tx, rx) = std::sync::mpsc::channel();
    app.git.view.tree_watch = TreeWatcher::from_receiver(rx);
    // Never enter Tree mode.
    assert_eq!(app.git.view.mode, ViewMode::Status);

    std::fs::create_dir(Path::new(&path).join("docs")).unwrap();
    tx.send(Ok(Vec::new())).unwrap();
    app.poll_tree_watcher();

    // The event is drained but must not build/touch the tree off-screen.
    assert_eq!(app.git.view.mode, ViewMode::Status);
    assert!(app.git.view.tree.cache.is_empty());
    drop(dir);
}

#[test]
fn leaving_tree_for_log_clears_watches() {
    let (dir, path) = make_tree_repo();
    let mut app = app_on(&path);
    app.enter_tree_mode();
    assert!(
        app.git.view.tree_watch.watched_count() > 0,
        "entering Tree mode watches at least the root"
    );

    app.toggle_mode(); // Tree -> Log via <prefix> l
    assert_eq!(app.git.view.mode, ViewMode::Log);
    assert_eq!(
        app.git.view.tree_watch.watched_count(),
        0,
        "leaving Tree for Log must drop all watches"
    );
    drop(dir);
}

#[test]
fn leaving_tree_for_status_clears_watches() {
    let (dir, path) = make_tree_repo();
    let mut app = app_on(&path);
    app.enter_tree_mode();
    assert!(app.git.view.tree_watch.watched_count() > 0);

    app.exit_tree_to_status();
    assert_eq!(app.git.view.mode, ViewMode::Status);
    assert_eq!(app.git.view.tree_watch.watched_count(), 0);
    drop(dir);
}

#[test]
fn toggle_mode_from_tree_enters_log_view() {
    let (dir, path) = make_tree_repo();
    let mut app = app_on(&path);
    app.enter_tree_mode();

    // `<prefix> l` from Tree goes to Log (not back to Status).
    app.toggle_mode();
    assert_eq!(app.git.view.mode, ViewMode::Log);
    drop(dir);
}
