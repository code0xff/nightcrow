use super::*;

#[test]
fn manager_constructor_owns_repository_runtime_state() {
    let (snapshot, _snapshot_tx) = dummy_snapshot_channel();
    let (tree_watch, _tree_tx) = dummy_tree_watcher();
    let manager = GitViewManager::from_test_parts("repo".to_string(), snapshot, tree_watch);

    assert_eq!(manager.repo_path(), "repo");
    assert!(manager.pending_snapshot().is_none());
    assert_eq!(manager.view().mode(), ViewMode::Status);
}

#[test]
fn adopting_an_id_does_not_replace_repository_view_state() {
    let (snapshot, _snapshot_tx) = dummy_snapshot_channel();
    let (tree_watch, _tree_tx) = dummy_tree_watcher();
    let mut manager = GitViewManager::from_test_parts("repo".to_string(), snapshot, tree_watch);
    manager.view_mut().status_mut().selected = 4;

    manager.adopt_repo_id("opaque-id".to_string());

    assert_eq!(manager.repo_id(), Some("opaque-id"));
    assert_eq!(manager.view().status().selected, 4);
}
