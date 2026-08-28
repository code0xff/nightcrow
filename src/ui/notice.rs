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
    repo_input: &'a RepoInput,
    accent: Color,
    width: u16,
) -> Paragraph<'a> {
    // The open dialog takes the header's row whole: the header names the repo
    // being left, the input names the one being opened. Notices follow the
    // dialog down to the hint row (`repo_dialog_hint_line`) so nothing covers
    // the path being typed.
    if repo_input.active {
        return Paragraph::new(crate::ui::repo_dialog::repo_input_line(
            repo_input, accent, width,
        ));
    }
    match notice_or_candidates(app.notice.as_ref(), repo_input, Some(&app.repo_path), width) {
        Some(line) => Paragraph::new(line),
        None => render_repo_header(app, accent, width),
    }
}

/// The row's content when something wants to claim it: a notice first, then
/// the repo dialog's completion candidates. `None` leaves the row to the
/// caller's own fallback. A notice outranks the candidates because it explains
/// a rejected action, and any edit clears it, so the two rarely compete.
/// With a `repo_path` present the path is kept and the notice truncates
/// with `…` when the pair exceeds `width`.
pub(crate) fn notice_or_candidates<'a>(
    notice: Option<&'a Notice>,
    repo_input: &RepoInput,
    repo_path: Option<&str>,
    width: u16,
) -> Option<Line<'a>> {
    if let Some(notice) = notice {
        let notice_text = format!(" {}", notice.line());
        let notice_style = Style::default().fg(Color::Red).add_modifier(Modifier::BOLD);

        if let Some(path) = repo_path {
            let display_path = home_relative_path(path);
            let path_str = format!(" {display_path} ");
            let path_style = Style::default()
                .fg(Color::Gray)
                .add_modifier(Modifier::BOLD);
            let path_width = Span::raw(&path_str).width();
            let notice_width = Span::raw(&notice_text).width();

            if path_width + notice_width <= width as usize {
                return Some(Line::from(vec![
                    Span::styled(path_str, path_style),
                    Span::styled(notice_text, notice_style),
                ]));
            }

            let available = (width as usize).saturating_sub(path_width);
            // One column is enough for the ellipsis alone: a notice cut to
            // nothing must still say it was there, as `+N more` does.
            if available == 0 {
                return Some(Line::from(vec![Span::styled(path_str, path_style)]));
            }
            let truncated = truncate_with_ellipsis(&notice_text, available);
            return Some(Line::from(vec![
                Span::styled(path_str, path_style),
                Span::styled(truncated, notice_style),
            ]));
        }

        return Some(Line::from(Span::styled(notice_text, notice_style)));
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

/// Truncate `text` to fit within `max_width` columns, appending `…` when cut.
///
/// Width is summed per character, so a sequence whose width is not the sum of
/// its parts — a variation selector, a combining mark — can come out a column
/// over. Re-measuring after each character would make this row quadratic in
/// its own width on every frame, for a column the terminal clips anyway.
fn truncate_with_ellipsis(text: &str, max_width: usize) -> String {
    if Span::raw(text).width() <= max_width {
        return text.to_string();
    }
    let ellipsis = "\u{2026}";
    let ellipsis_width = Span::raw(ellipsis).width();
    if max_width <= ellipsis_width {
        return ellipsis.to_string();
    }
    let target = max_width - ellipsis_width;
    let mut result = String::with_capacity(text.len());
    let mut w = 0usize;
    for ch in text.chars() {
        let cw = Span::raw(ch.to_string()).width();
        if w + cw > target {
            break;
        }
        w += cw;
        result.push(ch);
    }
    result.push_str(ellipsis);
    result
}

/// How much of the room left the branch may take when the path wants it as
/// well. The web footer splits it the same way (`RepoShell.tsx`), so the same
/// repository reads the same on both screens.
const BRANCH_NAME_SHARE: usize = 2;

/// The path and the branch as the row can hold them, cut with `…` rather than
/// pushed off the end. Both give way before the counts behind them, because
/// those counts do not: a name at full length would take the row from `↑N ↓M`
/// and the recovery chip, the part of this row that is news. The branch is
/// held to half of `budget` so a long one does not take the path's place
/// entirely, and dropped when half is nothing — an ellipsis alone names no
/// branch.
pub(crate) fn fit_names(
    path: &str,
    branch: Option<&str>,
    budget: usize,
) -> (String, Option<String>) {
    // Nothing left is nothing shown. `truncate_with_ellipsis` never returns
    // less than the ellipsis, which on a row this full is a column taken from
    // the chip it was making room for.
    if budget == 0 {
        return (String::new(), None);
    }
    let path = format!(" {path} ");
    let Some(branch) = branch else {
        return (truncate_with_ellipsis(&path, budget), None);
    };
    let branch = format!(" {branch} ");
    let share = (budget / BRANCH_NAME_SHARE).min(Span::raw(&branch).width());
    if share == 0 {
        return (truncate_with_ellipsis(&path, budget), None);
    }
    let branch = truncate_with_ellipsis(&branch, share);
    let left = budget.saturating_sub(Span::raw(&branch).width());
    (truncate_with_ellipsis(&path, left), Some(branch))
}

pub(crate) fn render_repo_header<'a>(app: &'a App, accent: Color, width: u16) -> Paragraph<'a> {
    let tracking = app
        .tracking
        .as_ref()
        .filter(|t| t.ahead > 0 || t.behind > 0)
        .map(|t| format!(" ^{} v{} ", t.ahead, t.behind));
    let chip = recovery_chip(app);
    // The counts and the chip keep their room: each is short, and each says
    // something no other row does.
    let kept: usize = [tracking.as_deref(), chip.as_deref()]
        .into_iter()
        .flatten()
        .map(|text| Span::raw(text).width())
        .sum();
    let (path, branch) = fit_names(
        &home_relative_path(&app.repo_path),
        app.branch_name.as_deref(),
        (width as usize).saturating_sub(kept),
    );

    let mut spans: Vec<Span<'a>> = vec![Span::styled(
        path,
        Style::default()
            .fg(Color::Gray)
            .add_modifier(Modifier::BOLD),
    )];
    if let Some(branch) = branch {
        spans.push(Span::styled(
            branch,
            Style::default().fg(accent).add_modifier(Modifier::BOLD),
        ));
    }
    if let Some(tracking) = tracking {
        spans.push(Span::styled(tracking, Style::default().fg(Color::Cyan)));
    }
    if let Some(chip) = chip {
        spans.push(Span::styled(
            chip,
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
        ));
    }
    Paragraph::new(Line::from(spans))
}

/// The full recovery report as one chip, on this row rather than a row of its
/// own for the reason the notices are: a row that appears and disappears
/// resizes every open PTY. It is the last chip, so an actual notice still
/// covers the whole line — a rejected action needs explaining more than a
/// wait does. The pane it describes is the one `<leader> c` would cancel.
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
    // Rebuilding from components removes a cosmetic trailing separator without
    // turning filesystem roots (`/`, `C:\\`, UNC shares) into different paths.
    let normalized: std::path::PathBuf = std::path::Path::new(path).components().collect();
    let path = normalized.as_path();
    if let Some(home) = dirs::home_dir()
        && let Ok(rest) = path.strip_prefix(home)
    {
        if rest.as_os_str().is_empty() {
            return "~".to_owned();
        }
        return format!("~/{}", crate::platform::paths::for_display(rest));
    }
    crate::platform::paths::for_display(path).into_owned()
}
