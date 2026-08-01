use super::helpers::*;
use crate::app::tests::app_with_files;
use crate::application::input::dispatch::{KeyOutcome, ProjectRequest, dispatch_key, handle_key};
use crate::workspace::Workspace;
use crossterm::event::{KeyCode, KeyModifiers};

#[test]
fn f_key_asks_the_workspace_to_switch_project() {
    let mut app = app_with_files(vec!["a.rs"]);

    // Bare F-keys need no prefix, and the request is emitted rather than
    // acted on: the handler holds one project and cannot reach the tabs.
    let outcome = handle_key(&mut app, press(KeyCode::F(3), KeyModifiers::NONE));

    assert_eq!(outcome, KeyOutcome::Project(ProjectRequest::Switch(2)));
}

#[test]
fn confirming_the_dialog_asks_the_workspace_to_open_that_path() {
    let (_dir, path) = crate::test_util::make_repo();
    let mut ws = workspace_on(&["/a"]);
    ws.start_repo_input();
    // Typing extends the prefill, so an unrelated absolute path starts by
    // backspacing the field empty — the gesture a user makes.
    while !ws.repo_input.buf.is_empty() {
        ws.repo_input_pop();
    }
    for c in path.chars() {
        ws.repo_input_push(c);
    }

    let outcome = dispatch_key(&mut ws, press(KeyCode::Enter, KeyModifiers::NONE));

    // The emitted path is the *resolved* workdir, not the typed text —
    // on macOS the temp dir reaches it through a /var -> /private/var
    // symlink, and the workdir carries a trailing separator.
    let expected = crate::git::resolve_repo_path(std::path::Path::new(&path))
        .to_string_lossy()
        .to_string();
    assert_eq!(outcome, KeyOutcome::Project(ProjectRequest::Open(expected)));
    // The current project still points at its original repo: confirming
    // opens a tab, it never repoints this one.
    assert_eq!(ws.active().unwrap().repo_path, "/a");
    assert!(!ws.repo_input.active, "dialog must close on success");
}

#[test]
fn the_dialog_completes_the_path_on_tab() {
    // Tab used to fall through to `text_input_char`, which rejects it, so the
    // key was silently swallowed. This pins the routing, not the completion
    // rules — those are covered in `workspace::path_complete`.
    let dir = tempfile::TempDir::new().expect("a temp dir");
    std::fs::create_dir(dir.path().join("nightcrow")).expect("create dir");
    let base = format!("{}/", dir.path().to_str().expect("a UTF-8 temp path"));
    let mut ws = workspace_on(&["/a"]);
    ws.start_repo_input();
    // Typing extends the prefill, so an unrelated absolute path starts by
    // backspacing the field empty — the gesture a user makes.
    while !ws.repo_input.buf.is_empty() {
        ws.repo_input_pop();
    }
    for c in format!("{base}night").chars() {
        ws.repo_input_push(c);
    }

    let outcome = dispatch_key(&mut ws, press(KeyCode::Tab, KeyModifiers::NONE));

    assert_eq!(outcome, KeyOutcome::Continue);
    assert_eq!(ws.repo_input.buf, format!("{base}nightcrow/"));
}

#[test]
fn confirming_the_dialog_on_a_bad_path_keeps_it_open() {
    let mut ws = workspace_on(&["/a"]);
    ws.start_repo_input();
    for c in "/definitely/not/a/directory".chars() {
        ws.repo_input_push(c);
    }

    let outcome = dispatch_key(&mut ws, press(KeyCode::Enter, KeyModifiers::NONE));

    assert_eq!(outcome, KeyOutcome::Continue);
    assert!(ws.repo_input.active, "a rejected path must stay editable");
}

#[test]
fn the_empty_screen_opens_the_dialog_and_quits() {
    let mut ws = Workspace::new(leader());
    assert!(ws.active().is_none());

    // The leader still arms with no project, and only `o` and `q` resolve.
    let _ = dispatch_key(&mut ws, leader());
    let open = dispatch_key(&mut ws, press(KeyCode::Char('o'), KeyModifiers::NONE));
    assert_eq!(open, KeyOutcome::Project(ProjectRequest::OpenDialog));

    let _ = dispatch_key(&mut ws, leader());
    let quit = dispatch_key(&mut ws, press(KeyCode::Char('q'), KeyModifiers::NONE));
    assert_eq!(quit, KeyOutcome::Quit);

    // An unbound follow-up is consumed, not forwarded anywhere.
    let _ = dispatch_key(&mut ws, leader());
    let other = dispatch_key(&mut ws, press(KeyCode::Char('t'), KeyModifiers::NONE));
    assert_eq!(other, KeyOutcome::Continue);
}

#[test]
fn dialog_swallows_the_leader_instead_of_arming_the_prefix() {
    let mut ws = workspace_on(&["/a"]);
    ws.start_repo_input();
    ws.repo_input.buf.clear();

    let _ = dispatch_key(&mut ws, leader());

    // The dispatcher gives the dialog every key, so the leader is typed
    // (and rejected as a control char) rather than arming a prefix behind
    // the modal.
    assert!(!ws.active().unwrap().interaction.prefix_armed);
    assert!(ws.repo_input.active);
}

#[test]
fn dialog_rejects_command_modifier_chars() {
    let mut ws = workspace_on(&["/a"]);
    ws.start_repo_input();
    ws.repo_input.buf.clear();

    let alt_x = press(KeyCode::Char('x'), KeyModifiers::ALT);
    let _ = dispatch_key(&mut ws, alt_x);

    assert!(ws.repo_input.buf.is_empty());
}
