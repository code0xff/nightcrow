use super::*;

#[test]
fn inert_watcher_tracks_desired_set_without_os_calls() {
    let (_tx, rx) = mpsc::channel();
    let mut w = TreeWatcher::from_receiver(rx);
    let root = Path::new("/tmp/repo");
    let mut desired = BTreeSet::new();
    desired.insert(String::new());
    desired.insert("src".to_string());
    w.sync(root, &desired);
    assert_eq!(w.watched, desired);

    // Reconcile down to just the root.
    let mut smaller = BTreeSet::new();
    smaller.insert(String::new());
    w.sync(root, &smaller);
    assert_eq!(w.watched, smaller);
}

fn event_at(path: &str) -> notify_debouncer_mini::DebouncedEvent {
    notify_debouncer_mini::DebouncedEvent {
        path: PathBuf::from(path),
        kind: notify_debouncer_mini::DebouncedEventKind::Any,
    }
}

/// A watcher already synced to `/tmp/repo`, so event paths can be made
/// repo-relative.
fn watcher_on_root(rx: Receiver<DebounceEventResult>) -> TreeWatcher {
    let mut w = TreeWatcher::from_receiver(rx);
    w.root = Some(PathBuf::from("/tmp/repo"));
    w
}

#[test]
fn drain_changed_reports_the_directories_that_changed_and_clears() {
    let (tx, rx) = mpsc::channel();
    let mut w = watcher_on_root(rx);
    assert!(w.drain_changed().is_empty(), "no events yet");

    // A file event is attributed to its parent — that is the listing whose
    // contents changed, and the one that has to be re-read.
    tx.send(Ok(vec![event_at("/tmp/repo/src/main.rs")]))
        .unwrap();
    tx.send(Ok(vec![event_at("/tmp/repo/README.md")])).unwrap();

    let changes = w.drain_changed();
    assert!(!changes.unknown);
    assert_eq!(
        changes.dirs,
        BTreeSet::from(["src".to_string(), String::new()]),
        "the repo root is the empty relative path"
    );
    // Drained: a second poll with nothing new reports no change.
    assert!(w.drain_changed().is_empty());
}

#[test]
fn drain_changed_flags_a_watcher_error_as_unknown() {
    // Events may have been dropped, so no set of directories can be
    // trusted to be complete — the caller must re-read wholesale.
    let (tx, rx) = mpsc::channel();
    let mut w = watcher_on_root(rx);
    tx.send(Err(notify::Error::generic("boom"))).unwrap();

    let changes = w.drain_changed();

    assert!(changes.unknown);
    assert!(!changes.is_empty());
}

#[test]
fn drain_changed_flags_a_path_outside_the_worktree_as_unknown() {
    let (tx, rx) = mpsc::channel();
    let mut w = watcher_on_root(rx);
    tx.send(Ok(vec![event_at("/elsewhere/file.rs")])).unwrap();

    let changes = w.drain_changed();

    assert!(changes.unknown, "an unmappable path cannot be attributed");
    assert!(changes.dirs.is_empty());
}

#[test]
fn join_rel_maps_root_and_subdir() {
    let root = Path::new("/tmp/repo");
    assert_eq!(join_rel(root, ""), PathBuf::from("/tmp/repo"));
    assert_eq!(join_rel(root, "src/ui"), PathBuf::from("/tmp/repo/src/ui"));
}
