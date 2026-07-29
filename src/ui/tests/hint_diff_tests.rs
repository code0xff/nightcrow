use super::common::*;
use crate::app::tests::app_with_fake_backend;
use crate::app::{DiffPaneView, Focus, ViewMode};
use crate::git::diff::StatusKind;

#[test]
fn normal_hint_advertises_close_only_with_terminal_focus() {
    let mut app = app_with_fake_backend();
    for focus in [Focus::FileList, Focus::DiffViewer] {
        app.focus = focus;
        let text = hint_text(&app);
        assert!(
            !text.contains("w: close pane"),
            "{focus:?} legend must not offer close, got: {text}"
        );
    }
    app.focus = Focus::Terminal;
    assert!(
        hint_text(&app).contains("w: close pane"),
        "terminal legend must offer close"
    );
}

/// `v` only opens a file when `current_file_view_key` resolves (log view
/// needs a drill-down file selection), so the diff legend must only
/// advertise `v: view file` then — a hint for a no-op key would lie.
#[test]
fn diff_hint_advertises_view_file_only_with_a_file_target() {
    // Log view browsing commits (no drill-down): `v` has no target.
    let mut app = app_with_fake_backend();
    app.mode = ViewMode::Log;
    app.focus = Focus::DiffViewer;
    let text = hint_text(&app);
    assert!(
        !text.contains("v: view file"),
        "commit-level log legend must not offer view file, got: {text}"
    );
    assert!(
        text.contains("s: split"),
        "split still acts on the commit diff, got: {text}"
    );

    // Same state zoomed: the fullscreen legend must agree.
    app.diff.fullscreen = true;
    let text = hint_text(&app);
    assert!(
        !text.contains("v: view file"),
        "zoomed commit-level legend must not offer view file, got: {text}"
    );

    // Drill-down with a file selected: `v` acts, so advertise it.
    app.diff.fullscreen = false;
    app.log_view
        .set_commits(vec![crate::git::diff::CommitEntry::new(
            git2::Oid::ZERO_SHA1,
            "deadbee".to_string(),
            "c".to_string(),
            "T".to_string(),
            0,
        )]);
    app.log_view.drill_down = true;
    app.log_view.commit_files = vec![crate::git::diff::ChangedFile::unstaged_only(
        "a.rs".to_string(),
        StatusKind::Modified,
    )];
    assert!(
        hint_text(&app).contains("v: view file"),
        "drill-down legend must offer view file"
    );

    // Status view with a selected file (the fixture's default list).
    let mut status = app_with_fake_backend();
    status.focus = Focus::DiffViewer;
    assert!(
        hint_text(&status).contains("v: view file"),
        "status legend must offer view file for a selected file"
    );
}

/// `w` is handled for the whole diff focus, so every view it acts in has to
/// advertise it — the file view most of all, since a long unwrapped line is
/// what sends you looking for the key. The split view is the one exception:
/// wrapping is ignored there, and a hint for a no-op key would lie.
#[test]
fn every_view_that_wraps_advertises_the_key() {
    let mut app = app_with_fake_backend();
    app.focus = Focus::DiffViewer;
    for view in [DiffPaneView::Diff, DiffPaneView::File] {
        app.diff.view = view;
        for zoomed in [false, true] {
            app.diff.fullscreen = zoomed;
            let text = hint_text(&app);
            assert!(
                text.contains("w: wrap"),
                "{view:?} legend (zoomed={zoomed}) must offer wrap, got: {text}"
            );
        }
    }

    // Tree mode's right pane is permanently the file view, and wraps the same.
    let mut tree = app_with_fake_backend();
    tree.mode = ViewMode::Tree;
    tree.focus = Focus::DiffViewer;
    tree.diff.view = DiffPaneView::File;
    assert!(hint_text(&tree).contains("w: wrap"), "tree file view wraps");

    app.diff.view = DiffPaneView::Split;
    app.diff.fullscreen = false;
    let text = hint_text(&app);
    assert!(
        !text.contains("w: wrap"),
        "the split view ignores wrapping, so it must not offer it, got: {text}"
    );
}

/// Tree mode's right pane is permanently the file view — `v` never
/// toggles there, so the file-view legend must not offer `back to diff`.
#[test]
fn tree_file_view_hint_omits_back_to_diff() {
    let mut app = app_with_fake_backend();
    app.mode = ViewMode::Tree;
    app.focus = Focus::DiffViewer;
    app.diff.view = DiffPaneView::File;
    let text = hint_text(&app);
    assert!(
        !text.contains("v: back to diff"),
        "tree file-view legend must not offer back to diff, got: {text}"
    );

    app.diff.fullscreen = true;
    let text = hint_text(&app);
    assert!(
        !text.contains("v: back to diff"),
        "zoomed tree file-view legend must not offer back to diff, got: {text}"
    );
}
