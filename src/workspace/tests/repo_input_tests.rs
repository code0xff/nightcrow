use super::common::*;
use super::*;
use crate::app::tests::app_with_files;

#[test]
fn typing_extends_the_prefilled_repo_path() {
    let mut ws = workspace_on(&["/repos/current"]);
    ws.start_repo_input();
    assert_eq!(ws.repo_input.buf, "/repos/current");

    for c in "/sub".chars() {
        ws.repo_input_push(c);
    }

    assert_eq!(
        ws.repo_input.buf, "/repos/current/sub",
        "typing must extend the prefill, never replace it"
    );
}

#[test]
fn backspace_edits_the_path_without_dropping_it() {
    let mut ws = workspace_on(&["/repos/current"]);
    ws.start_repo_input();

    ws.repo_input_pop();
    assert_eq!(ws.repo_input.buf, "/repos/curren");
    ws.repo_input_push('t');
    assert_eq!(
        ws.repo_input.buf, "/repos/current",
        "after Backspace, typing must append to the surviving text"
    );
}

#[test]
fn confirming_a_tilde_path_opens_the_home_relative_directory() {
    // The dialog never passes through a shell, so an unexpanded `~` would
    // be rejected as "no such directory".
    let home = dirs::home_dir().expect("a home directory");
    let mut ws = workspace_on(&["/repos/current"]);
    ws.start_repo_input();
    // Typing extends the prefill, so an unrelated absolute path starts by
    // backspacing the field empty — the gesture a user makes.
    while !ws.repo_input.buf.is_empty() {
        ws.repo_input_pop();
    }
    for c in "~".chars() {
        ws.repo_input_push(c);
    }

    let result = ws.confirm_repo_input();

    assert_eq!(
        result,
        RepoInputResult::Open(
            crate::git::resolve_repo_path(&home)
                .to_string_lossy()
                .to_string()
        )
    );
}

/// A workspace whose prefill is `<temp dir>/<frag>`, so Tab has a real
/// directory to complete against.
fn workspace_completing_in(root: &tempfile::TempDir, frag: &str) -> (Workspace, String) {
    let base = format!("{}/", root.path().to_str().expect("a UTF-8 temp path"));
    let mut ws = workspace_on(&[&format!("{base}{frag}")]);
    ws.start_repo_input();
    (ws, base)
}

#[test]
fn completing_extends_the_prefill() {
    let root = tempfile::TempDir::new().expect("a temp dir");
    std::fs::create_dir(root.path().join("nightcrow")).expect("create dir");
    let (mut ws, base) = workspace_completing_in(&root, "night");

    ws.repo_input_complete();

    assert_eq!(ws.repo_input.buf, format!("{base}nightcrow/"));
}

#[test]
fn completing_at_a_directory_boundary_offers_the_subdirectories() {
    let root = tempfile::TempDir::new().expect("a temp dir");
    std::fs::create_dir(root.path().join("alpha")).expect("create dir");
    std::fs::create_dir(root.path().join("beta")).expect("create dir");
    let (mut ws, base) = workspace_completing_in(&root, "");

    ws.repo_input_complete();

    assert_eq!(ws.repo_input.buf, base, "nothing shared to extend");
    assert_eq!(ws.repo_input.candidates, vec!["alpha", "beta"]);
}

#[test]
fn editing_after_a_completion_drops_the_stale_candidate_list() {
    let root = tempfile::TempDir::new().expect("a temp dir");
    std::fs::create_dir(root.path().join("alpha")).expect("create dir");
    std::fs::create_dir(root.path().join("beta")).expect("create dir");
    let (mut ws, _) = workspace_completing_in(&root, "");
    ws.repo_input_complete();
    assert_eq!(ws.repo_input.candidates.len(), 2);

    ws.repo_input_push('a');

    assert!(
        ws.repo_input.candidates.is_empty(),
        "the list described a fragment the buffer no longer holds"
    );
}

#[test]
fn completing_a_path_that_matches_nothing_raises_no_notice() {
    // Mid-typing, a path that does not exist yet is the normal state — only
    // confirming one is an error.
    let root = tempfile::TempDir::new().expect("a temp dir");
    let (mut ws, base) = workspace_completing_in(&root, "zzz");

    ws.repo_input_complete();

    assert_eq!(ws.repo_input.buf, format!("{base}zzz"));
    assert!(ws.repo_input.candidates.is_empty());
    assert!(ws.empty_notice().is_none());
}

#[test]
fn cancelling_discards_the_edit_and_reopening_starts_from_the_repo_path() {
    let mut ws = workspace_on(&["/repos/current"]);
    ws.start_repo_input();
    ws.repo_input_push('x');
    ws.cancel_repo_input();

    ws.start_repo_input();

    assert_eq!(
        ws.repo_input.buf, "/repos/current",
        "Esc is how a path is discarded, so the next open is clean"
    );
}

#[test]
fn 프로젝트가_없으면_다이얼로그가_빈_상태로_열린다() {
    let mut ws = Workspace::new(test_leader());

    ws.start_repo_input();

    assert!(ws.repo_input.active);
    assert_eq!(ws.repo_input.buf, "", "no project to prefill from");
}

#[test]
fn 프로젝트를_열면_빈_화면_공지가_사라진다() {
    // Otherwise a stale rejection would reappear the moment the last tab
    // was closed again, long after it stopped being true.
    let mut ws = Workspace::new(test_leader());
    ws.raise_notice(NoticeKind::RepoInput, "no such directory");

    ws.add(project_at("/a"));
    ws.close_repo("/a");

    assert!(ws.active().is_none(), "back to the empty screen");
    assert!(ws.empty_notice().is_none());
}

#[test]
fn 프로젝트가_없으면_공지가_workspace에_남는다() {
    let mut ws = Workspace::new(test_leader());

    ws.raise_notice(NoticeKind::RepoInput, "no such directory");

    assert_eq!(
        ws.empty_notice().map(|n| n.text.as_str()),
        Some("no such directory")
    );
    ws.clear_notice(NoticeKind::RepoInput);
    assert!(ws.empty_notice().is_none());
}

#[test]
fn 새_workspace는_프로젝트_하나를_활성으로_갖는다() {
    let ws = workspace_from(app_with_files(vec!["a.rs"]));

    assert_eq!(ws.projects().len(), 1);
    assert_eq!(ws.active().unwrap().repo_path, ".");
}
