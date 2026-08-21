use super::common::*;
use crate::app::App;
use crate::app::NoticeKind;
use crate::app::tests::{app_with_fake_backend, app_with_files};

#[cfg(windows)]
#[test]
fn repo_header_preserves_a_windows_drive_root() {
    assert_eq!(crate::ui::notice::home_relative_path(r"C:\"), "C:/");
}

#[test]
fn repo_input_reports_a_rejected_path_on_the_hint_row() {
    let mut ws = test_workspace();
    ws.start_repo_input();
    ws.repo_input.buf = "/definitely/not/here".to_string();
    ws.confirm_repo_input();

    assert!(
        ws.repo_input.active,
        "a rejected path must leave the dialog open for correction"
    );
    let repo_input = ws.repo_input.clone();
    // The input holds the notice row while the dialog is open, so the
    // rejection lands on the hint row, directly below the text to correct.
    let notice = notice_text_with(ws.active().unwrap(), &repo_input);
    assert!(
        notice.contains("/definitely/not/here"),
        "the rejected text must stay in the input, got: {notice}"
    );
    let hint = hint_text_with(ws.active().unwrap(), plain_chrome(&repo_input));
    assert!(
        hint.contains("no such directory"),
        "the hint row must say why the confirm was rejected, got: {hint}"
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
        (|app: &mut App| app.interaction.prefix_armed = true) as fn(&mut App),
        |app: &mut App| app.interaction.begin_swap_target(),
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
fn completion_candidates_take_the_hint_row_over_the_legend() {
    let app = app_with_files(vec![]);
    let dialog = dialog_offering(&["nightcrow", "nightowl"]);

    let text = hint_text_with(&app, plain_chrome(&dialog));

    assert!(text.contains("nightcrow"), "got: {text}");
    assert!(text.contains("nightowl"), "got: {text}");
    assert!(
        !text.contains("tab: complete"),
        "the candidates answer the Tab that is on screen, got: {text}"
    );
}

/// A notice explains a rejected action, so it outranks a candidate list — and
/// because any edit clears it, the two cannot both be stale for long.
#[test]
fn a_notice_outranks_the_completion_candidates() {
    let mut app = app_with_files(vec![]);
    app.raise_notice(NoticeKind::RepoInput, "no such directory");
    let dialog = dialog_offering(&["nightcrow"]);

    let text = hint_text_with(&app, plain_chrome(&dialog));

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
                crate::ui::hint_bar::render_hint_bar(
                    &app,
                    plain_chrome(&dialog),
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
    let during = notice_text(&app);
    assert!(
        during.contains("/tmp/somewhere"),
        "path must stay visible alongside notice, got: {during}"
    );
    assert!(
        during.contains("boom"),
        "notice text must be visible, got: {during}"
    );

    app.clear_notice(NoticeKind::Tree);
    assert_eq!(notice_text(&app), before);
}

/// A notice raised on a project with a repo path shows both the path and the
/// notice text on the same line.
#[test]
fn 공지가_뜨면_저장소_경로도_함께_보인다() {
    let mut app = app_with_files(vec![]);
    app.repo_path = "/tmp/my-project".to_string();
    app.raise_notice(NoticeKind::Git, "not a git repository");

    let text = notice_text(&app);
    assert!(
        text.contains("/tmp/my-project"),
        "repo path must be visible, got: {text}"
    );
    assert!(
        text.contains("git error: not a git repository"),
        "notice text must be visible, got: {text}"
    );
}

/// When the terminal is too narrow to fit both path and notice, the path is
/// kept and the notice is truncated with `…`.
#[test]
fn 좁은_너비에서는_경로가_남고_공지가_잘린다() {
    let mut app = app_with_files(vec![]);
    app.repo_path = "/tmp/p".to_string();
    app.raise_notice(NoticeKind::Git, "not a git repository");

    let mut terminal =
        ratatui::Terminal::new(ratatui::backend::TestBackend::new(20, 1)).expect("a terminal");
    terminal
        .draw(|frame| {
            frame.render_widget(
                crate::ui::notice::render_notice_row(
                    &app,
                    &crate::ui::status_view::RepoInput::default(),
                    ratatui::style::Color::Yellow,
                    frame.area().width,
                ),
                frame.area(),
            )
        })
        .expect("draw");
    let buf = terminal.backend().buffer();
    let text: String = (0..buf.area.width).map(|x| buf[(x, 0)].symbol()).collect();

    assert!(text.contains("/tmp/p"), "path must be visible, got: {text}");
    assert!(
        text.contains('\u{2026}'),
        "truncated notice must show ellipsis, got: {text}"
    );
}

/// Even with only one column left over, the notice is not dropped silently:
/// the ellipsis alone says something was cut.
#[test]
fn 공지_자리가_한_칸뿐이어도_잘렸다는_표시는_남는다() {
    let mut app = app_with_files(vec![]);
    app.repo_path = "/tmp/p".to_string();
    app.raise_notice(NoticeKind::Git, "not a git repository");

    // `/tmp/p` renders as " /tmp/p ", so 9 columns leave exactly one.
    let mut terminal =
        ratatui::Terminal::new(ratatui::backend::TestBackend::new(9, 1)).expect("a terminal");
    terminal
        .draw(|frame| {
            frame.render_widget(
                crate::ui::notice::render_notice_row(
                    &app,
                    &crate::ui::status_view::RepoInput::default(),
                    ratatui::style::Color::Yellow,
                    frame.area().width,
                ),
                frame.area(),
            )
        })
        .expect("draw");
    let buf = terminal.backend().buffer();
    let text: String = (0..buf.area.width).map(|x| buf[(x, 0)].symbol()).collect();

    assert!(text.contains("/tmp/p"), "path must be visible, got: {text}");
    assert!(
        text.contains('\u{2026}'),
        "the cut notice must still be marked, got: {text}"
    );
}

/// The open dialog replaces the repo header entirely: the header names the
/// repo being left, the input names the one being opened.
#[test]
fn 다이얼로그가_열리면_입력이_저장소_헤더를_대체한다() {
    let mut app = app_with_files(vec![]);
    app.repo_path = "/tmp/somewhere".to_string();

    let text = notice_text_with(&app, &dialog_offering(&["nightcrow", "nightowl"]));

    assert!(text.contains("repo: /repos/"), "got: {text}");
    assert!(
        !text.contains("/tmp/somewhere"),
        "the input must replace the repo header, got: {text}"
    );
    assert!(
        !text.contains("nightcrow"),
        "the candidates belong to the hint row, got: {text}"
    );
}

/// No notice and no candidates shows the repo header (no regression).
#[test]
fn 공지도_후보도_없으면_저장소_헤더가_보인다() {
    let mut app = app_with_files(vec![]);
    app.repo_path = "/tmp/somewhere".to_string();
    let text = notice_text(&app);
    assert!(text.contains("/tmp/somewhere"), "got: {text}");
}
