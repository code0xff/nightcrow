use super::*;

#[test]
fn repository_view_keeps_shared_diff_when_switching_modes() {
    let mut view = RepositoryView::default();
    view.diff_mut().file_view = seeded_file_view("src/lib.rs");

    view.set_mode(ViewMode::Tree);
    view.set_mode(ViewMode::Status);

    assert_eq!(
        view.diff().file_view.key,
        Some(FileViewKey::Status("src/lib.rs".to_string()))
    );
}

#[test]
fn pending_selection_belongs_to_repository_view() {
    let mut view = RepositoryView::default();
    view.set_pending_selection(Some(("src/main.rs".to_string(), 7)));

    assert_eq!(
        view.take_pending_selection(),
        Some(("src/main.rs".to_string(), 7))
    );
    assert!(view.take_pending_selection().is_none());
}
