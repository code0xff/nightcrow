use super::common::*;
use super::*;
use crate::app::tests::app_with_files;

#[test]
fn first_typed_char_replaces_the_prefilled_repo_path() {
    let mut ws = workspace_on(&["/repos/current"]);
    ws.start_repo_input();
    assert_eq!(ws.repo_input.buf, "/repos/current");

    for c in "/tmp".chars() {
        ws.repo_input_push(c);
    }

    assert_eq!(
        ws.repo_input.buf, "/tmp",
        "typing over an untouched prefill must replace it, not append"
    );
}

#[test]
fn backspace_leaves_prefill_mode_without_dropping_the_path() {
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
fn accepting_the_prefill_appends_instead_of_replacing() {
    let mut ws = workspace_on(&["/repos/current/"]);
    ws.start_repo_input();

    ws.repo_input_accept_prefill();
    for c in "src".chars() {
        ws.repo_input_push(c);
    }

    assert_eq!(ws.repo_input.buf, "/repos/current/src");
}

#[test]
fn confirming_a_tilde_path_opens_the_home_relative_directory() {
    // The dialog never passes through a shell, so an unexpanded `~` would
    // be rejected as "no such directory".
    let home = dirs::home_dir().expect("a home directory");
    let mut ws = workspace_on(&["/repos/current"]);
    ws.start_repo_input();
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

#[test]
fn reopening_the_dialog_re_arms_the_prefill() {
    let mut ws = workspace_on(&["/repos/current"]);
    ws.start_repo_input();
    ws.repo_input_push('x');
    ws.cancel_repo_input();

    ws.start_repo_input();
    ws.repo_input_push('y');
    assert_eq!(ws.repo_input.buf, "y");
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
