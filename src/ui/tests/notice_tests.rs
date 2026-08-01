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

fn dialog_offering(candidates: &[&str]) -> crate::ui::status_view::RepoInput {
    crate::ui::status_view::RepoInput {
        active: true,
        buf: "/repos/".to_string(),
        candidates: candidates.iter().map(|c| c.to_string()).collect(),
        picker: None,
    }
}

#[test]
fn completion_candidates_take_the_notice_row_over_repo_identity() {
    let mut app = app_with_files(vec![]);
    app.repo_path = "/tmp/somewhere".to_string();

    let text = notice_text_with(&app, &dialog_offering(&["nightcrow", "nightowl"]));

    assert!(text.contains("nightcrow"), "got: {text}");
    assert!(text.contains("nightowl"), "got: {text}");
    assert!(
        !text.contains("/tmp/somewhere"),
        "the candidates answer the Tab that is on screen, got: {text}"
    );
}

/// A notice explains a rejected action, so it outranks a candidate list — and
/// because any edit clears it, the two cannot both be stale for long.
#[test]
fn a_notice_outranks_the_completion_candidates() {
    let mut app = app_with_files(vec![]);
    app.raise_notice(NoticeKind::RepoInput, "no such directory");

    let text = notice_text_with(&app, &dialog_offering(&["nightcrow"]));

    assert!(text.contains("no such directory"), "got: {text}");
    assert!(!text.contains("nightcrow"), "got: {text}");
}

#[test]
fn a_candidate_list_too_wide_for_the_row_reports_what_it_dropped() {
    let mut terminal =
        ratatui::Terminal::new(ratatui::backend::TestBackend::new(24, 1)).expect("a terminal");
    let app = app_with_files(vec![]);
    let dialog = dialog_offering(&["alpha", "bravo", "charlie", "delta"]);

    terminal
        .draw(|frame| {
            frame.render_widget(
                crate::ui::notice::render_notice_row(
                    &app,
                    &dialog,
                    ratatui::style::Color::Yellow,
                    frame.area().width,
                ),
                frame.area(),
            )
        })
        .expect("draw");
    let buf = terminal.backend().buffer();
    let text: String = (0..buf.area.width).map(|x| buf[(x, 0)].symbol()).collect();

    assert!(text.contains("alpha"), "got: {text}");
    assert!(
        text.contains("more"),
        "a truncated list must say the tail exists, got: {text}"
    );
    assert!(
        !text.contains("delta"),
        "the row is 24 columns wide, got: {text}"
    );
}

#[test]
fn the_empty_screen_shows_completion_candidates_too() {
    // With no project there is no repo header to fall back to, so the row is
    // free — but it still has to render the list.
    let text = drawn_empty(&dialog_offering(&["nightcrow", "nightowl"]), None, false);

    assert!(text.contains("nightcrow"), "got: {text}");
    assert!(text.contains("nightowl"), "got: {text}");
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
