//! Tree `Enter` (`tree_open_selected`): opens a file fullscreen, ignores dirs.

use super::tree::{app_on, make_tree_repo, tree_index_of};
use super::*;

#[test]
fn tree_open_on_directory_row_does_not_change_expansion() {
    let (dir, path) = make_tree_repo();
    let mut app = app_on(&path);
    app.enter_tree_mode();
    app.tree_view.selected = tree_index_of(&app, "src");

    app.tree_open_selected();
    assert!(
        !app.tree_view.expanded.contains("src"),
        "Enter must not expand a directory"
    );
    assert!(
        !app.diff.fullscreen,
        "a directory row must not zoom the pane"
    );

    // Already expanded: Enter must not collapse it either.
    app.tree_expand();
    app.tree_view.selected = tree_index_of(&app, "src");
    app.tree_open_selected();
    assert!(
        app.tree_view.expanded.contains("src"),
        "Enter must not collapse a directory"
    );
    drop(dir);
}

#[test]
fn tree_open_on_file_row_loads_file_view_and_goes_fullscreen() {
    let (dir, path) = make_tree_repo();
    let mut app = app_on(&path);
    app.enter_tree_mode();
    app.tree_view.selected = tree_index_of(&app, "README.md");

    app.tree_open_selected();
    app.flush_git_loads_for_test(Duration::from_secs(2));

    assert_eq!(app.diff.view, DiffPaneView::File);
    assert_eq!(
        app.diff.file_view.key,
        Some(FileViewKey::Status("README.md".to_string()))
    );
    assert_eq!(app.diff.file_view.content, "# hi\n");
    assert!(app.diff.fullscreen);
    assert_eq!(app.focus, Focus::DiffViewer);
    drop(dir);
}

#[test]
fn tree_open_on_file_row_clears_competing_fullscreens() {
    let (dir, path) = make_tree_repo();
    let mut app = app_on(&path);
    app.enter_tree_mode();
    app.list_fullscreen = true;
    app.terminal.fullscreen = TerminalFullscreen::Grid;
    app.tree_view.selected = tree_index_of(&app, "README.md");

    app.tree_open_selected();

    assert!(app.diff.fullscreen);
    assert!(!app.list_fullscreen);
    assert_eq!(app.terminal.fullscreen, TerminalFullscreen::Off);
    drop(dir);
}
