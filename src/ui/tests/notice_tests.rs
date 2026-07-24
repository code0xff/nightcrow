use super::common::*;
use crate::app::App;
use crate::app::NoticeKind;
use crate::app::tests::{app_with_fake_backend, app_with_files};

#[test]
fn repo_input_reports_a_rejected_path_on_the_notice_row() {
    let mut ws = test_workspace();
    ws.start_repo_input();
    ws.repo_input.buf = "/definitely/not/here".to_string();
    ws.confirm_repo_input();

    assert!(
        ws.repo_input.active,
        "a rejected path must leave the dialog open for correction"
    );
    // With a project open the rejection lands on that project's notice row,
    // directly above the input still holding the text to correct.
    let notice = notice_text(ws.active().unwrap());
    assert!(
        notice.contains("no such directory"),
        "the notice row must say why the confirm was rejected, got: {notice}"
    );
    let repo_input = ws.repo_input.clone();
    let hint = hint_text_with(ws.active().unwrap(), plain_chrome(&repo_input));
    assert!(
        hint.contains("/definitely/not/here"),
        "the rejected text must stay in the input, got: {hint}"
    );
}

#[test]
fn repo_input_notice_clears_once_the_path_is_edited() {
    let mut ws = test_workspace();
    ws.start_repo_input();
    ws.repo_input.buf = "/definitely/not/here".to_string();
    ws.confirm_repo_input();
    ws.repo_input_pop();

    let notice = notice_text(ws.active().unwrap());
    assert!(
        !notice.contains("no such directory"),
        "editing the path must clear the stale verdict, got: {notice}"
    );
}

/// The notice row is the one place every kind reports, and no overlay may
/// shadow it — the hint bar's own early-returns are what made a notice
/// invisible before it moved off that row.
#[test]
fn notice_row_shows_notices_through_every_overlay() {
    for setup in [
        (|app: &mut App| app.arm_prefix()) as fn(&mut App),
        |app: &mut App| app.begin_swap_target(),
    ] {
        let mut app = app_with_fake_backend();
        setup(&mut app);
        app.raise_notice(NoticeKind::Git, "not a repo");
        let text = notice_text(&app);
        assert!(
            text.contains("git error: not a repo"),
            "an open overlay must not shadow the notice row, got: {text}"
        );
    }
}

/// With nothing raised the row is the repo/branch line, and it comes back
/// intact after a notice is cleared.
#[test]
fn notice_row_falls_back_to_repo_identity() {
    let mut app = app_with_files(vec![]);
    app.repo_path = "/tmp/somewhere".to_string();
    let before = notice_text(&app);
    assert!(before.contains("/tmp/somewhere"), "got: {before}");

    app.raise_notice(NoticeKind::Tree, "boom");
    assert!(!notice_text(&app).contains("/tmp/somewhere"));

    app.clear_notice(NoticeKind::Tree);
    assert_eq!(notice_text(&app), before);
}
