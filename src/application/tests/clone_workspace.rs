//! Clone mode remains available from the TUI's empty workspace.

use super::clone_dialog::{dialog, press_shift_tab};
use super::helpers::press;
use crate::application::input::dispatch::{KeyOutcome, ProjectRequest, dispatch_key};
use crate::ui::status_view::RepoInputMode;
use crate::workspace::Workspace;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

#[test]
fn an_empty_workspace_can_choose_a_parent_and_start_a_clone() {
    let leader = KeyEvent::new(KeyCode::Char('f'), KeyModifiers::CONTROL);
    let mut ws = Workspace::new(leader);
    ws.set_clone_available(true);
    ws.start_repo_input();
    assert!(ws.active().is_none());

    assert_eq!(
        dispatch_key(&mut ws, press(KeyCode::BackTab, KeyModifiers::SHIFT)),
        KeyOutcome::Project(ProjectRequest::CloneStatus { job: None })
    );
    assert_eq!(ws.repo_input.mode, RepoInputMode::CloneParent);
    assert!(std::path::Path::new(&ws.repo_input.clone_parent).is_absolute());
    ws.apply_clone_lookup(crate::session::CloneLookup::Idle);
    dispatch_key(&mut ws, press(KeyCode::Enter, KeyModifiers::NONE));
    ws.repo_input.clone_url = "https://example.com/team/empty-screen.git".into();

    assert!(matches!(
        dispatch_key(&mut ws, press(KeyCode::Enter, KeyModifiers::NONE)),
        KeyOutcome::Project(ProjectRequest::StartClone { .. })
    ));
    assert!(ws.active().is_none());
}

#[test]
fn hidden_owned_and_adopted_results_wait_for_explicit_open() {
    let mut hidden = dialog();
    hidden.repo_input.mode = RepoInputMode::CloneUrl;
    hidden.repo_input.clone_status = crate::ui::status_view::CloneUiStatus::Running;
    hidden.repo_input.clone_job = Some(8);
    hidden.repo_input.clone_owned = true;
    hidden.repo_input.clone_started_visible = true;
    hidden.cancel_repo_input();
    assert_eq!(
        hidden.apply_clone_lookup(crate::session::CloneLookup::Found {
            job: 8,
            state: crate::session::CloneJobState::Done {
                path: "/tmp/new".into(),
            },
        }),
        None
    );
    assert!(!hidden.repo_input.active);
    assert_eq!(hidden.active().unwrap().repository_path(), "/existing");

    let mut adopted = dialog();
    adopted.repo_input.mode = RepoInputMode::CloneUrl;
    adopted.repo_input.clone_status = crate::ui::status_view::CloneUiStatus::Running;
    adopted.repo_input.clone_job = Some(9);
    assert_eq!(
        adopted.apply_clone_lookup(crate::session::CloneLookup::Found {
            job: 9,
            state: crate::session::CloneJobState::Done {
                path: "/tmp/adopted".into(),
            },
        }),
        None
    );
    assert!(adopted.repo_input.active);
    assert_eq!(
        dispatch_key(&mut adopted, press(KeyCode::Enter, KeyModifiers::NONE)),
        KeyOutcome::Project(ProjectRequest::Open("/tmp/adopted".into()))
    );
}

#[test]
fn reopening_a_hidden_completed_clone_restores_its_explicit_open_action() {
    let mut ws = dialog();
    ws.repo_input.mode = RepoInputMode::CloneUrl;
    ws.repo_input.clone_status = crate::ui::status_view::CloneUiStatus::Done("/tmp/new".into());
    ws.cancel_repo_input();
    ws.start_repo_input();

    press_shift_tab(&mut ws);

    assert_eq!(ws.repo_input.mode, RepoInputMode::CloneUrl);
    assert_eq!(
        dispatch_key(&mut ws, press(KeyCode::Enter, KeyModifiers::NONE)),
        KeyOutcome::Project(ProjectRequest::Open("/tmp/new".into()))
    );
}

#[test]
fn lost_start_reply_after_fast_completion_is_uncertain_not_failed() {
    let mut ws = dialog();
    ws.repo_input.mode = RepoInputMode::CloneUrl;
    ws.repo_input.clone_url = "https://example.com/team/repo.git".into();
    ws.repo_input.clone_status = crate::ui::status_view::CloneUiStatus::Starting;
    ws.repo_input.clone_owned = true;
    ws.repo_input.clone_started_visible = true;

    assert_eq!(
        ws.apply_clone_lookup(crate::session::CloneLookup::Idle),
        None
    );

    assert_eq!(
        ws.repo_input.clone_status,
        crate::ui::status_view::CloneUiStatus::Unconfirmed
    );
    assert_eq!(ws.repo_input.clone_url, "https://example.com/team/repo.git");
    assert!(!ws.repo_input.clone_owned);
}

#[test]
fn failed_open_preserves_result_and_confirmed_open_allows_a_second_clone() {
    let mut ws = dialog();
    ws.set_clone_available(true);
    ws.repo_input.mode = RepoInputMode::CloneUrl;
    ws.repo_input.clone_url = "https://example.com/team/first.git".into();
    ws.repo_input.clone_status = crate::ui::status_view::CloneUiStatus::Done("/tmp/first".into());

    assert_eq!(
        dispatch_key(&mut ws, press(KeyCode::Enter, KeyModifiers::NONE)),
        KeyOutcome::Project(ProjectRequest::Open("/tmp/first".into()))
    );
    assert!(ws.repo_input.clone_open_pending.is_some());
    assert!(ws.clone_open_failed());
    assert_eq!(
        ws.repo_input.clone_status,
        crate::ui::status_view::CloneUiStatus::Done("/tmp/first".into())
    );
    assert_eq!(
        ws.repo_input.clone_url,
        "https://example.com/team/first.git"
    );

    assert!(ws.clone_open_requested("/tmp/first"));
    assert!(ws.clone_open_confirmed(&["/tmp/first".into()]));
    assert_eq!(
        ws.repo_input.clone_status,
        crate::ui::status_view::CloneUiStatus::Idle
    );
    assert!(ws.repo_input.clone_url.is_empty());

    ws.start_repo_input();
    assert_eq!(
        press_shift_tab(&mut ws),
        KeyOutcome::Project(ProjectRequest::CloneStatus { job: None })
    );
    assert_eq!(ws.repo_input.mode, RepoInputMode::CloneParent);
    ws.apply_clone_lookup(crate::session::CloneLookup::Idle);
    ws.repo_input.clone_parent = std::env::current_dir()
        .unwrap()
        .to_string_lossy()
        .into_owned();
    dispatch_key(&mut ws, press(KeyCode::Enter, KeyModifiers::NONE));
    ws.repo_input.clone_url = "https://example.com/team/second.git".into();

    assert!(matches!(
        dispatch_key(&mut ws, press(KeyCode::Enter, KeyModifiers::NONE)),
        KeyOutcome::Project(ProjectRequest::StartClone { url, .. }) if url.ends_with("second.git")
    ));
}

#[test]
fn a_visible_owned_clone_returns_its_path_for_auto_open() {
    let mut ws = dialog();
    ws.repo_input.mode = RepoInputMode::CloneUrl;
    ws.repo_input.clone_status = crate::ui::status_view::CloneUiStatus::Running;
    ws.repo_input.clone_job = Some(10);
    ws.repo_input.clone_owned = true;
    ws.repo_input.clone_started_visible = true;

    assert_eq!(
        ws.apply_clone_lookup(crate::session::CloneLookup::Found {
            job: 10,
            state: crate::session::CloneJobState::Done {
                path: "/tmp/new".into(),
            },
        }),
        Some("/tmp/new".into())
    );
    assert!(!ws.repo_input.active);
}
