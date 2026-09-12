use super::common::*;
use crate::ui::repo_dialog::{render_repo_input_row, repo_dialog_hint_line, repo_input_line};
use crate::ui::status_view::RepoInput;
use crate::workspace::PathTree;
use ratatui::{Terminal, backend::TestBackend, style::Color};
use std::time::Instant;
use tempfile::TempDir;

/// The dialog's state with the browser open on a temp directory holding `dirs`.
fn browsing(dirs: &[&str]) -> (TempDir, RepoInput) {
    let root = TempDir::new().expect("a temp dir");
    for d in dirs {
        std::fs::create_dir(root.path().join(d)).expect("create dir");
    }
    let buf = std::fs::canonicalize(root.path())
        .expect("canonical temp path")
        .to_str()
        .expect("a UTF-8 temp path")
        .to_string();
    let picker = PathTree::open(&buf).expect("a readable root");
    (
        root,
        RepoInput {
            active: true,
            buf,
            candidates: Vec::new(),
            picker: Some(picker),
            ..RepoInput::default()
        },
    )
}

fn field_only() -> RepoInput {
    RepoInput {
        active: true,
        buf: "/repos/current".to_string(),
        candidates: Vec::new(),
        picker: None,
        ..RepoInput::default()
    }
}

#[test]
fn the_browser_fills_the_body_with_the_directories_it_read() {
    let (_guard, repo_input) = browsing(&["alpha", "zeta"]);

    let screen = drawn_empty(&repo_input, None, false);

    assert!(
        screen.contains("browse"),
        "the box names where it is:\n{screen}"
    );
    assert!(
        screen.contains("alpha") && screen.contains("zeta"),
        "{screen}"
    );
}

#[test]
fn an_empty_directory_says_so_rather_than_drawing_a_blank_box() {
    let (_guard, repo_input) = browsing(&[]);

    let screen = drawn_empty(&repo_input, None, false);

    assert!(screen.contains("no sub-directories"), "{screen}");
}

#[test]
fn the_dialog_advertises_its_keys_on_the_hint_row() {
    // Nothing else can: the dialog replaces the hint legend entirely, so an
    // unadvertised key is unfindable.
    let field = field_only();
    let line = repo_dialog_hint_line(None, &field, 90).to_string();

    assert!(
        line.contains("down: browse"),
        "the way into the browser: {line}"
    );
    assert!(line.contains("tab: complete"), "{line}");
}

#[test]
fn the_browsers_own_keys_replace_the_fields_on_the_hint_row() {
    let (_guard, repo_input) = browsing(&["alpha"]);

    let line = repo_dialog_hint_line(None, &repo_input, 200).to_string();

    assert!(line.contains("enter: open"), "{line}");
    assert!(line.contains("left: up"), "{line}");
    assert!(!line.contains("down: browse"), "already browsing: {line}");
}

#[test]
fn the_input_row_carries_the_path_and_the_caret_and_nothing_else() {
    // The legend lives on its own row now, so even a path that would once
    // have crowded it out shares its line with nothing.
    let mut field = field_only();
    field.buf = "/a".repeat(30);

    let line = repo_input_line(&field, Color::Yellow, 90).to_string();

    assert!(line.contains(&field.buf), "the path itself: {line}");
    assert!(
        !line.contains("browse"),
        "the legend belongs to the hint row: {line}"
    );
    assert!(line.ends_with('|'), "the caret has to survive: {line}");
}

#[test]
fn a_path_longer_than_the_row_shows_its_tail_and_keeps_the_caret() {
    // The caret marks where typing lands, so it is the end of the path that
    // must survive — the front folds into a `…` instead of pushing the caret
    // off the row.
    let mut field = field_only();
    field.buf = format!("{}/the-repo", "/a".repeat(30));

    let line = repo_input_line(&field, Color::Yellow, 40).to_string();

    assert!(line.ends_with("/the-repo|"), "got: {line}");
    assert!(line.contains('\u{2026}'), "the cut must be marked: {line}");
    assert!(
        ratatui::text::Span::raw(line.as_str()).width() <= 40,
        "the line must fit the row it was cut for: {line}"
    );
}

#[test]
fn a_tail_holding_a_width_shifting_sequence_still_fits_the_row() {
    // Per-character width sums miss context-sensitive sequences (an emoji
    // with a variation selector); the invariant is what matters: whatever the
    // tail holds, the built line never exceeds the row it was cut for.
    let mut field = field_only();
    field.buf = format!("{}\u{2764}\u{FE0F}end", "/a".repeat(30));

    let line = repo_input_line(&field, Color::Yellow, 40).to_string();

    assert!(line.ends_with("end|"), "the caret has to survive: {line}");
    assert!(
        ratatui::text::Span::raw(line.as_str()).width() <= 40,
        "the line must fit the row it was cut for: {line}"
    );
}

#[test]
fn a_row_too_narrow_for_any_path_keeps_the_prompt_and_caret_alone() {
    // ` repo: ` + `|` is 8 columns; at exactly that width no path fits, and
    // an ellipsis would itself push the caret off the end it protects.
    let field = field_only();

    let line = repo_input_line(&field, Color::Yellow, 8).to_string();

    assert_eq!(line, " repo: |", "got: {line}");
}

#[test]
fn the_focus_border_keeps_its_rail_while_pulsing_twice() {
    let mut field = field_only();
    let started = Instant::now();
    field.start_focus_flash_at(started);
    let mut rendered = Vec::new();

    for phase in 0u32..=4 {
        field.advance_focus_flash_at(
            started + crate::ui::status_view::REPO_INPUT_FLASH_PHASE * phase,
        );
        let mut terminal = Terminal::new(TestBackend::new(40, 1)).expect("a terminal");
        terminal
            .draw(|frame| {
                frame.render_widget(
                    render_repo_input_row(&field, Color::Yellow, frame.area().width),
                    frame.area(),
                )
            })
            .expect("draw");
        let buf = terminal.backend().buffer();
        let text = (0..buf.area.width)
            .map(|x| buf[(x, 0)].symbol())
            .collect::<String>();
        rendered.push((text, buf[(0, 0)].style().fg));
    }

    assert!(rendered.iter().all(|(text, _)| text.starts_with('│')));
    assert!(rendered.iter().all(|(text, _)| text.ends_with('│')));
    assert!(
        rendered
            .iter()
            .all(|(text, _)| ratatui::text::Span::raw(text).width() == 40)
    );
    assert_eq!(
        rendered.iter().map(|(_, color)| *color).collect::<Vec<_>>(),
        vec![
            Some(Color::Yellow),
            Some(Color::DarkGray),
            Some(Color::Yellow),
            Some(Color::DarkGray),
            Some(Color::DarkGray),
        ]
    );
}

#[test]
fn a_rejection_takes_the_dialogs_hint_row_over_the_legend() {
    // The same priority the notice row applies when the dialog is closed: the
    // rejection explains the enter that just did nothing, and any edit clears
    // it, so the legend is never gone for long.
    let field = field_only();
    let notice = crate::app::Notice::new(crate::app::NoticeKind::RepoInput, "no such directory");

    let line = repo_dialog_hint_line(Some(&notice), &field, 90).to_string();

    assert!(line.contains("no such directory"), "{line}");
    assert!(!line.contains("tab: complete"), "{line}");
}
