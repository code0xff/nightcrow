//! Key routing for the repo dialog's directory browser. Pins which surface owns
//! a key — the browser's own behaviour lives in `workspace::path_tree`, and the
//! field/browser contract in `workspace::tests::repo_picker_tests`.

use super::helpers::*;
use crate::application::input::dispatch::{KeyOutcome, ProjectRequest, dispatch_key};
use crate::workspace::Workspace;
use crossterm::event::{KeyCode, KeyModifiers};
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
    let canonical = std::fs::canonicalize(root.path()).expect("canonical temp path");
    // Strip `\\\\?\\` and normalise to forward slashes so the path can
    // round-trip through `PathTree::open` and test assertions are consistent.
    #[cfg(windows)]
    let text = {
        let s = canonical.to_string_lossy();
        let stripped = s.strip_prefix(r"\\?\").unwrap_or(&s);
        stripped.replace('\\', "/")
    };
    #[cfg(not(windows))]
    let text = canonical.to_str().expect("a UTF-8 temp path").to_string();
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
fn a_second_tab_stays_in_the_field_without_opening_the_browser() {
    // Tab only completes the path; the browser opens with ↓ alone. So a
    // second Tab when the list is already up just re-runs completion — it
    // must never leave the field.
    let (_guard, mut ws, _) = dialog_on(&["alpha", "another"]);
    ws.repo_input.buf.push('/');

    send(&mut ws, KeyCode::Tab);
    assert!(
        !ws.repo_input.candidates.is_empty() && ws.repo_input.picker.is_none(),
        "the first Tab lists without leaving the field"
    );

    send(&mut ws, KeyCode::Tab);
    assert!(
        ws.repo_input.picker.is_none(),
        "the second Tab stays in the field — the browser opens with ↓ only"
    );
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

    // The tree nav keys never reach the buffer — only Enter does, and it goes
    // straight to opening rather than editing the field.
    assert!(ws.repo_input.picker.is_some(), "still browsing");
    assert_eq!(ws.repo_input.buf, text);
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

/// One Enter on a row asks the workspace to open that directory: pick and
/// confirm were two keys for one gesture, so the dialog closes behind the
/// request and the browser never returns to the field.
#[test]
fn enter_on_a_row_opens_that_directory_in_one_key() {
    let (guard, mut ws, text) = dialog_on(&["alpha"]);
    send(&mut ws, KeyCode::Down);

    // The one Enter both picks the row and asks the workspace to open it.
    let outcome = dispatch_key(&mut ws, press(KeyCode::Enter, KeyModifiers::NONE));

    let KeyOutcome::Project(ProjectRequest::Open(path)) = outcome else {
        panic!("the Enter must ask the workspace to open: {outcome:?}");
    };
    let resolved = crate::git::resolve_repo_path(std::path::Path::new(&path))
        .to_string_lossy()
        .to_string();
    let expected = crate::git::resolve_repo_path(std::path::Path::new(&format!(
        "{}/alpha/",
        text.trim_end_matches('/')
    )))
    .to_string_lossy()
    .to_string();
    assert_eq!(resolved, expected);
    assert!(
        !ws.repo_input.active,
        "the dialog closed behind the request"
    );
    let _ = guard;
}

#[test]
fn enter_on_an_empty_directory_opens_the_root_itself() {
    let (_guard, mut ws, _text) = dialog_on(&[]);
    send(&mut ws, KeyCode::Down);

    let outcome = dispatch_key(&mut ws, press(KeyCode::Enter, KeyModifiers::NONE));

    // No rows to select, so Enter hands the root itself to the field's confirm.
    let KeyOutcome::Project(ProjectRequest::Open(_)) = outcome else {
        panic!("the root must be openable: {outcome:?}");
    };
    assert!(
        !ws.repo_input.active,
        "the dialog closed behind the request"
    );
}
