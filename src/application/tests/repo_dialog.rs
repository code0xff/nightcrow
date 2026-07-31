//! Key routing for the repo dialog's directory browser. Pins which surface owns
//! a key — the browser's own behaviour lives in `workspace::path_tree`, and the
//! field/browser contract in `workspace::tests::repo_picker_tests`.

use super::helpers::*;
use crate::application::input::dispatch::{KeyOutcome, dispatch_key};
use crate::workspace::Workspace;
use crossterm::event::{KeyCode, KeyModifiers};
use std::path::MAIN_SEPARATOR;
use tempfile::TempDir;

/// The dialog open on a real temp directory holding `dirs`, plus its canonical
/// path as the field's text.
fn dialog_on(dirs: &[&str]) -> (TempDir, Workspace, String) {
    let root = TempDir::new().expect("a temp dir");
    for d in dirs {
        let mut p = root.path().to_path_buf();
        for part in d.split('/') {
            p.push(part);
            if !p.is_dir() {
                std::fs::create_dir(&p).expect("create dir");
            }
        }
    }
    let text = std::fs::canonicalize(root.path())
        .expect("canonical temp path")
        .to_str()
        .expect("a UTF-8 temp path")
        .to_string();
    let mut ws = workspace_on(&["/a"]);
    ws.start_repo_input();
    ws.repo_input.buf = text.clone();
    (root, ws, text)
}

fn send(ws: &mut Workspace, code: KeyCode) {
    assert_eq!(
        dispatch_key(ws, press(code, KeyModifiers::NONE)),
        KeyOutcome::Continue
    );
}

#[test]
fn down_in_the_field_opens_the_browser() {
    let (_guard, mut ws, _) = dialog_on(&["alpha"]);

    send(&mut ws, KeyCode::Down);

    assert!(ws.repo_input.picker.is_some());
}

#[test]
fn a_second_tab_escalates_from_the_candidate_list_to_the_browser() {
    // The first Tab cannot extend past the shared prefix, so it lists; the
    // second would repeat that list, which is the moment the list proved too
    // little.
    let (_guard, mut ws, _) = dialog_on(&["alpha", "another"]);
    ws.repo_input.buf.push('/');

    send(&mut ws, KeyCode::Tab);
    assert!(
        !ws.repo_input.candidates.is_empty() && ws.repo_input.picker.is_none(),
        "the first Tab lists without leaving the field"
    );

    send(&mut ws, KeyCode::Tab);
    assert!(ws.repo_input.picker.is_some(), "the second Tab escalates");
}

#[test]
fn tab_still_completes_when_no_list_is_up() {
    let (_guard, mut ws, text) = dialog_on(&["alpha"]);
    ws.repo_input.buf = format!("{text}/al");

    send(&mut ws, KeyCode::Tab);

    assert_eq!(ws.repo_input.buf, format!("{text}/alpha/"));
    assert!(ws.repo_input.picker.is_none());
}

#[test]
fn the_browser_takes_the_keys_the_field_would_have_had() {
    let (_guard, mut ws, text) = dialog_on(&["alpha/inner"]);
    send(&mut ws, KeyCode::Down);

    // In the field these would edit the buffer; here they drive the tree.
    send(&mut ws, KeyCode::Right);
    send(&mut ws, KeyCode::Down);
    send(&mut ws, KeyCode::Enter);

    assert!(ws.repo_input.picker.is_none(), "Enter selects and returns");
    assert_eq!(ws.repo_input.buf, format!("{text}{MAIN_SEPARATOR}alpha{MAIN_SEPARATOR}inner{MAIN_SEPARATOR}"));
    assert!(ws.repo_input.active, "selecting must not open the repo");
}

#[test]
fn typing_cannot_change_the_field_while_the_browser_is_up() {
    let (_guard, mut ws, text) = dialog_on(&["alpha"]);
    send(&mut ws, KeyCode::Down);

    send(&mut ws, KeyCode::Char('x'));
    send(&mut ws, KeyCode::Backspace);

    assert_eq!(ws.repo_input.buf, text);
}

#[test]
fn the_first_esc_leaves_the_browser_and_the_second_cancels_the_dialog() {
    let (_guard, mut ws, _) = dialog_on(&["alpha"]);
    send(&mut ws, KeyCode::Down);

    send(&mut ws, KeyCode::Esc);
    assert!(ws.repo_input.picker.is_none());
    assert!(ws.repo_input.active, "the field survives the first Esc");

    send(&mut ws, KeyCode::Esc);
    assert!(!ws.repo_input.active);
}

#[test]
fn j_and_k_move_the_browser_without_reaching_the_field() {
    let (_guard, mut ws, text) = dialog_on(&["alpha", "zeta"]);
    send(&mut ws, KeyCode::Down);

    send(&mut ws, KeyCode::Char('j'));
    assert_eq!(
        ws.repo_input.picker.as_ref().expect("open").selected(),
        1,
        "`j` moves the cursor rather than typing a `j`"
    );
    send(&mut ws, KeyCode::Char('k'));
    send(&mut ws, KeyCode::Enter);

    assert_eq!(ws.repo_input.buf, format!("{text}{MAIN_SEPARATOR}alpha{MAIN_SEPARATOR}"));
}
