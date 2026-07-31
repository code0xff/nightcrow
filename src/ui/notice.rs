use crate::app::{App, Notice};
use crate::ui::status_view::RepoInput;
use crate::ui::wall_clock::local_hour_minute;
use ratatui::{
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::Paragraph,
};

/// Separates candidate names, and sits before the overflow count.
const CANDIDATE_GAP: &str = "  ";

pub(crate) fn render_notice_row<'a>(
    app: &'a App,
    repo_input: &RepoInput,
    accent: Color,
    width: u16,
) -> Paragraph<'a> {
    match notice_or_candidates(app.notice.as_ref(), repo_input, width) {
        Some(line) => Paragraph::new(line),
        None => render_repo_header(app, accent),
    }
}

/// The notice row's content when something wants to claim it: a notice first,
/// then the repo dialog's completion candidates. `None` leaves the row to the
/// caller's own fallback — the repo header on the project screen, nothing on the
/// empty one.
///
/// A notice outranks the candidates because it explains a rejected action, and
/// any edit (Tab included) clears it, so the two rarely compete for long.
pub(crate) fn notice_or_candidates<'a>(
    notice: Option<&'a Notice>,
    repo_input: &RepoInput,
    width: u16,
) -> Option<Line<'a>> {
    if let Some(notice) = notice {
        return Some(Line::from(Span::styled(
            format!(" {}", notice.line()),
            Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
        )));
    }
    if repo_input.candidates.is_empty() {
        return None;
    }
    // Dim, not red: these are an answer to Tab, and reading them as an error
    // would undo the point of showing them.
    Some(Line::from(Span::styled(
        candidate_line(&repo_input.candidates, width),
        Style::default().fg(Color::DarkGray),
    )))
}

/// Fit as many candidate names as the row holds, reporting the rest as
/// `+N more`. The row is one line, so a long list has to be cut somewhere and
/// dropping the tail silently would read as "that is all there is".
fn candidate_line(candidates: &[String], width: u16) -> String {
    let width = width as usize;
    let mut line = String::new();
    let mut shown = 0;
    for name in candidates {
        let next = format!("{}{name}", if shown == 0 { " " } else { CANDIDATE_GAP });
        // Reserve room for the count this name would push into the overflow, so
        // the last name placed can never crowd out its own `+N more`.
        let overflow = overflow_label(candidates.len() - shown - 1);
        if Span::raw(&line).width() + Span::raw(&next).width() + Span::raw(&overflow).width()
            > width
        {
            break;
        }
        line.push_str(&next);
        shown += 1;
    }
    line.push_str(&overflow_label(candidates.len() - shown));
    line
}

fn overflow_label(remaining: usize) -> String {
    if remaining == 0 {
        return String::new();
    }
    format!("{CANDIDATE_GAP}+{remaining} more")
}

pub(crate) fn render_repo_header<'a>(app: &'a App, accent: Color) -> Paragraph<'a> {
    let display_path = home_relative_path(&app.repo_path);
    let mut spans: Vec<Span<'a>> = vec![Span::styled(
        format!(" {display_path} "),
        Style::default()
            .fg(Color::Gray)
            .add_modifier(Modifier::BOLD),
    )];
    if let Some(branch) = app.branch_name.as_deref() {
        spans.push(Span::styled(
            format!(" {branch} "),
            Style::default().fg(accent).add_modifier(Modifier::BOLD),
        ));
    }
    if let Some(t) = &app.tracking
        && (t.ahead > 0 || t.behind > 0)
    {
        spans.push(Span::styled(
            format!(" ↑{} ↓{} ", t.ahead, t.behind),
            Style::default().fg(Color::Cyan),
        ));
    }
    if let Some(chip) = recovery_chip(app) {
        spans.push(Span::styled(
            chip,
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
        ));
    }
    Paragraph::new(Line::from(spans))
}

/// The full recovery report as one chip: which pane, the plugin's state, the
/// deadline as a local wall-clock time, the attempts spent, and the detail line.
///
/// On this row rather than in a row or overlay of its own for the reason the
/// notices are: a row that appears and disappears resizes every open PTY. It is
/// the last chip, so an actual notice still covers the whole line — a rejected
/// action needs explaining more than a wait does. The pane it describes is the
/// one `<leader> c` would cancel (see `TerminalState::recovery_focus`).
fn recovery_chip(app: &App) -> Option<String> {
    let (pane, report) = app.terminal.recovery_focus()?;
    let mut chip = format!(" pane {pane}: {}", report.state);
    if let Some(at) = report.deadline_epoch.and_then(local_hour_minute) {
        chip.push_str(&format!(" until {at}"));
    }
    if report.attempt > 0 {
        chip.push_str(&format!(" (attempt {})", report.attempt));
    }
    if let Some(detail) = report.detail.as_deref() {
        chip.push_str(&format!(" — {detail}"));
    }
    chip.push(' ');
    Some(chip)
}

pub(crate) fn home_relative_path(path: &str) -> String {
    let trimmed = path.trim_end_matches('/');
    let display = crate::platform::paths::for_display(std::path::Path::new(trimmed));
    if let Some(home) = dirs::home_dir()
        && let Some(home_str) = home.to_str()
        && let Some(rest) = display.strip_prefix(home_str)
    {
        return format!("~{rest}");
    }
    display.into_owned()
}
