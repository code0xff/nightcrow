//! Input routing and focus policy for the repository dialog's Clone mode.

use super::helpers::{press, workspace_on};
use crate::application::input::dispatch::{KeyOutcome, ProjectRequest, dispatch_key};
use crate::ui::status_view::{CloneUiStatus, RepoInputMode};
use crate::workspace::Workspace;
use crossterm::event::{KeyCode, KeyModifiers};

pub(super) fn dialog() -> Workspace {
    let mut ws = workspace_on(&["/existing"]);
    ws.start_repo_input();
    ws
}

pub(super) fn press_shift_tab(ws: &mut Workspace) -> KeyOutcome {
    dispatch_key(ws, press(KeyCode::BackTab, KeyModifiers::SHIFT))
}

#[test]
fn shift_tab_switches_modes_without_replacing_open_input() {
    let mut ws = dialog();
    ws.set_clone_available(true);
    let open_path = ws.repo_input.buf.clone();

    assert_eq!(
        press_shift_tab(&mut ws),
        KeyOutcome::Project(ProjectRequest::CloneStatus { job: None })
    );
    assert_eq!(ws.repo_input.mode, RepoInputMode::CloneParent);
    ws.repo_input.clone_parent = "/tmp/parent".into();
    ws.repo_input.clone_url = "https://example.com/team/repo.git".into();
    press_shift_tab(&mut ws);

    assert_eq!(ws.repo_input.mode, RepoInputMode::Open);
    assert_eq!(ws.repo_input.buf, open_path);
    assert_eq!(ws.repo_input.clone_parent, "/tmp/parent");
    assert_eq!(ws.repo_input.clone_url, "https://example.com/team/repo.git");
}

#[test]
fn clone_mode_on_an_unavailable_daemon_emits_no_status_request() {
    let mut ws = dialog();

    assert_eq!(press_shift_tab(&mut ws), KeyOutcome::Continue);
    assert_eq!(ws.repo_input.mode, RepoInputMode::CloneParent);
    assert_eq!(ws.repo_input.clone_status, CloneUiStatus::Idle);
    assert_eq!(
        ws.take_clone_poll(std::time::Instant::now() + std::time::Duration::from_secs(1)),
        None
    );
}

#[test]
fn a_failed_url_can_return_to_parent_then_retry_only_from_the_url_stage() {
    let mut ws = dialog();
    ws.set_clone_available(true);
    ws.repo_input.mode = RepoInputMode::CloneUrl;
    ws.repo_input.clone_parent = std::env::current_dir()
        .unwrap()
        .to_string_lossy()
        .into_owned();
    ws.repo_input.clone_url = "https://example.com/team/repo.git".into();
    ws.clone_rejected("remote refused the request");

    assert_eq!(
        dispatch_key(&mut ws, press(KeyCode::Esc, KeyModifiers::NONE)),
        KeyOutcome::Continue
    );
    assert_eq!(ws.repo_input.mode, RepoInputMode::CloneParent);
    assert_eq!(
        dispatch_key(&mut ws, press(KeyCode::Enter, KeyModifiers::NONE)),
        KeyOutcome::Continue
    );
    assert_eq!(ws.repo_input.mode, RepoInputMode::CloneUrl);
    assert_eq!(
        ws.repo_input.clone_status,
        CloneUiStatus::Failed("remote refused the request".into())
    );

    assert!(matches!(
        dispatch_key(&mut ws, press(KeyCode::Enter, KeyModifiers::NONE)),
        KeyOutcome::Project(ProjectRequest::StartClone { .. })
    ));
}

#[test]
fn parent_browser_and_tab_completion_work_before_url_submission() {
    let root = tempfile::TempDir::new().unwrap();
    std::fs::create_dir(root.path().join("alpha")).unwrap();
    let mut ws = dialog();
    ws.set_clone_available(true);
    press_shift_tab(&mut ws);
    let base = root.path().to_string_lossy().into_owned();
    ws.repo_input.clone_parent = format!("{base}/al");

    dispatch_key(&mut ws, press(KeyCode::Tab, KeyModifiers::NONE));
    assert_eq!(ws.repo_input.clone_parent, format!("{base}/alpha/"));
    dispatch_key(&mut ws, press(KeyCode::Down, KeyModifiers::NONE));
    assert!(ws.repo_input.picker.is_some());
    dispatch_key(&mut ws, press(KeyCode::Enter, KeyModifiers::NONE));

    assert!(ws.repo_input.picker.is_none());
    assert_eq!(ws.repo_input.mode, RepoInputMode::CloneUrl);
    assert_eq!(
        std::path::Path::new(&ws.repo_input.clone_parent)
            .canonicalize()
            .unwrap(),
        root.path().join("alpha").canonicalize().unwrap()
    );
}

#[test]
fn url_paste_targets_only_the_url_field_and_strips_terminal_controls() {
    let mut ws = dialog();
    ws.set_clone_available(true);
    press_shift_tab(&mut ws);
    ws.repo_input.clone_parent = std::env::current_dir()
        .unwrap()
        .to_string_lossy()
        .into_owned();
    dispatch_key(&mut ws, press(KeyCode::Enter, KeyModifiers::NONE));
    crate::application::input::paste::dispatch_paste(
        &mut ws,
        "https://example.com/team/猫.git\u{1b}[31m",
    );

    assert!(ws.repo_input.buf.ends_with("/existing"));
    assert_eq!(
        ws.repo_input.clone_url,
        "https://example.com/team/猫.git[31m"
    );
}

#[test]
fn starting_and_running_jobs_ignore_duplicate_enter_and_escape_hides_only() {
    let mut ws = dialog();
    ws.repo_input.mode = RepoInputMode::CloneUrl;
    ws.repo_input.clone_status = CloneUiStatus::Running;
    ws.repo_input.clone_job = Some(7);
    ws.repo_input.clone_url = "https://example.com/team/repo.git".into();

    assert_eq!(
        dispatch_key(&mut ws, press(KeyCode::Enter, KeyModifiers::NONE)),
        KeyOutcome::Continue
    );
    dispatch_key(&mut ws, press(KeyCode::Esc, KeyModifiers::NONE));

    assert!(!ws.repo_input.active);
    assert_eq!(ws.repo_input.clone_status, CloneUiStatus::Running);
    assert_eq!(ws.repo_input.clone_job, Some(7));
}

#[test]
fn failure_keeps_fields_correctable_and_enter_retries() {
    let mut ws = dialog();
    ws.set_clone_available(true);
    ws.repo_input.mode = RepoInputMode::CloneUrl;
    ws.repo_input.clone_parent = std::env::current_dir()
        .unwrap()
        .to_string_lossy()
        .into_owned();
    ws.repo_input.clone_url = "https://example.com/team/repo.git".into();
    ws.clone_rejected("remote failed\u{1b}[31m");

    assert_eq!(
        ws.repo_input.clone_parent,
        std::env::current_dir().unwrap().to_string_lossy()
    );
    assert_eq!(ws.repo_input.clone_url, "https://example.com/team/repo.git");
    assert_eq!(
        ws.repo_input.clone_status,
        CloneUiStatus::Failed("remote failed [31m".into())
    );
    assert!(matches!(
        dispatch_key(&mut ws, press(KeyCode::Enter, KeyModifiers::NONE)),
        KeyOutcome::Project(ProjectRequest::StartClone { .. })
    ));
}
