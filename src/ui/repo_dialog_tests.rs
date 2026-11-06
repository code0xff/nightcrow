use super::*;
use crate::ui::status_view::{CloneUiStatus, RepoInputMode};
use ratatui::style::Color;

fn line_text(line: &Line<'_>) -> String {
    line.spans
        .iter()
        .map(|span| span.content.as_ref())
        .collect()
}

fn line_width(line: &Line<'_>) -> usize {
    line.spans.iter().map(Span::width).sum()
}

#[test]
fn running_clone_status_fits_a_narrow_row_with_wide_url_text() {
    let input = RepoInput {
        active: true,
        mode: RepoInputMode::CloneUrl,
        clone_url: "https://例え.テスト/team/猫.git".into(),
        clone_status: CloneUiStatus::Running,
        ..RepoInput::default()
    };

    let line = repo_input_line(&input, Color::Yellow, 28);

    assert!(line_text(&line).contains("cloning..."));
    assert!(line_width(&line) <= 28, "{}", line_text(&line));
}

#[test]
fn failure_hint_removes_terminal_controls_and_keeps_retry_visible() {
    let input = RepoInput {
        active: true,
        mode: RepoInputMode::CloneUrl,
        clone_status: CloneUiStatus::Failed("remote said \u{1b}[31mno route\nretry".into()),
        ..RepoInput::default()
    };

    let line = repo_dialog_hint_line(None, &input, 30);
    let text = line_text(&line);

    assert!(!text.contains('\u{1b}'), "{text:?}");
    assert!(text.starts_with("Enter retry"), "{text:?}");
    assert!(line_width(&line) <= 30, "{text:?}");
}

#[test]
fn completed_clone_result_keeps_the_open_action_and_unicode_path_readable() {
    let input = RepoInput {
        active: true,
        mode: RepoInputMode::CloneUrl,
        clone_status: CloneUiStatus::Done("C:/work/猫 project".into()),
        ..RepoInput::default()
    };

    let row = repo_input_line(&input, Color::Yellow, 32);
    let hint = repo_dialog_hint_line(None, &input, 32);

    assert!(line_text(&row).contains("猫 project"));
    assert!(line_text(&hint).contains("Enter opens"));
    assert!(line_width(&row) <= 32);
    assert!(line_width(&hint) <= 32);
}

#[test]
fn open_dialog_advertises_clone_as_a_local_mode_switch() {
    let input = RepoInput {
        active: true,
        ..RepoInput::default()
    };

    let hint = repo_dialog_hint_line(None, &input, 90);

    assert!(line_text(&hint).contains("shift+tab: clone"));
}

#[test]
fn a_long_open_path_keeps_its_visible_tail_and_caret() {
    let input = RepoInput {
        active: true,
        buf: format!("{}KEEP-THIS-TAIL", "a".repeat(3000)),
        ..RepoInput::default()
    };

    let line = repo_input_line(&input, Color::Yellow, 24);
    let text = line_text(&line);

    assert!(text.contains("KEEP-THIS-TAIL"), "{text}");
    assert!(text.ends_with('|'), "{text}");
    assert!(line_width(&line) <= 24);
}
