//! The contract between the path field and the directory browser: which one
//! holds the state, and what crossing between them does to the buffer.

use super::common::*;
use crate::app::NoticeKind;
use tempfile::TempDir;

/// A workspace whose dialog is open on a real temp directory, since the browser
/// reads the filesystem.
fn dialog_on(dirs: &[&str]) -> (TempDir, crate::workspace::Workspace) {
    let root = TempDir::new().expect("a temp dir");
    for d in dirs {
        std::fs::create_dir(root.path().join(d)).expect("create dir");
    }
    let canonical = std::fs::canonicalize(root.path()).expect("canonical temp path");
    let mut ws = workspace_on(&[canonical.to_str().expect("a UTF-8 temp path")]);
    ws.start_repo_input();
    (root, ws)
}

#[test]
fn browsing_opens_on_the_prefilled_path() {
    let (_guard, mut ws) = dialog_on(&["alpha"]);

    ws.repo_input_browse();

    let picker = ws.repo_input.picker.as_ref().expect("the browser is open");
    assert_eq!(picker.rows().len(), 1);
    assert_eq!(picker.rows()[0].name, "alpha");
}

#[test]
fn browsing_leaves_prefill_mode_so_the_picked_path_survives_typing() {
    let (_guard, mut ws) = dialog_on(&["alpha"]);

    ws.repo_input_browse();
    ws.repo_input_pick();
    ws.repo_input_push('x');

    assert!(
        ws.repo_input.buf.ends_with("alpha/x"),
        "typing must extend the picked path, not replace it: {}",
        ws.repo_input.buf
    );
}

#[test]
fn picking_a_row_closes_the_browser_and_fills_the_field() {
    let (_guard, mut ws) = dialog_on(&["alpha"]);
    let before = ws.repo_input.buf.clone();

    ws.repo_input_browse();
    ws.repo_input_pick();

    assert!(ws.repo_input.picker.is_none(), "back to the field");
    assert_eq!(ws.repo_input.buf, format!("{before}/alpha/"));
}

#[test]
fn closing_the_browser_keeps_the_text_it_started_from() {
    let (_guard, mut ws) = dialog_on(&["alpha"]);
    let before = ws.repo_input.buf.clone();

    ws.repo_input_browse();
    ws.repo_picker_move(true);
    ws.repo_input_close_browser();

    assert!(ws.repo_input.picker.is_none());
    assert_eq!(
        ws.repo_input.buf, before,
        "an abandoned browse must not have to be retyped"
    );
    assert!(ws.repo_input.active, "the dialog itself stays open");
}

#[test]
fn browsing_an_unreadable_path_reports_it_and_stays_in_the_field() {
    let (_guard, mut ws) = dialog_on(&[]);
    for c in "/missing/deeper".chars() {
        ws.repo_input_push(c);
    }

    ws.repo_input_browse();

    assert!(ws.repo_input.picker.is_none());
    assert_eq!(
        ws.active().and_then(|p| p.notice.as_ref()).map(|n| n.kind),
        Some(NoticeKind::RepoInput),
        "the refusal has to be visible somewhere"
    );
}

#[test]
fn browsing_drops_the_candidate_list_it_replaces() {
    let (_guard, mut ws) = dialog_on(&["alpha", "another"]);
    ws.repo_input_push('/');
    ws.repo_input_complete();
    assert!(!ws.repo_input.candidates.is_empty(), "two names to offer");

    ws.repo_input_browse();

    assert!(
        ws.repo_input.candidates.is_empty(),
        "the browser answers the same question the list did"
    );
}

#[test]
fn cancelling_the_dialog_takes_the_browser_with_it() {
    let (_guard, mut ws) = dialog_on(&["alpha"]);
    ws.repo_input_browse();

    ws.cancel_repo_input();

    assert!(ws.repo_input.picker.is_none());
    assert!(!ws.repo_input.active);
}

#[test]
fn navigation_is_inert_with_the_browser_closed() {
    let (_guard, mut ws) = dialog_on(&["alpha"]);
    let before = ws.repo_input.buf.clone();

    ws.repo_picker_move(true);
    ws.repo_picker_expand();
    ws.repo_picker_collapse();
    ws.repo_input_pick();

    assert_eq!(ws.repo_input.buf, before);
    assert!(ws.repo_input.picker.is_none());
}
