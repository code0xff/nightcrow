//! The repo dialog's two chrome rows: the input line on the notice row, its
//! keys and reports on the hint row.

use crate::app::Notice;
use crate::ui::notice::notice_or_candidates;
use crate::ui::status_view::{CloneUiStatus, RepoInput, RepoInputMode};
use ratatui::{
    style::{Color, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph},
};

/// Render the active field with a fixed two-cell side rail. Keeping the rail
/// in both phases means the path and caret never shift while focus is cued.
pub(crate) fn render_repo_input_row<'a>(
    repo_input: &'a RepoInput,
    accent: Color,
    width: u16,
) -> Paragraph<'a> {
    let border_color = if repo_input.focus_flash_bright() {
        accent
    } else {
        Color::DarkGray
    };
    Paragraph::new(repo_input_line(repo_input, accent, width.saturating_sub(2))).block(
        Block::default()
            .borders(Borders::LEFT | Borders::RIGHT)
            .border_style(Style::default().fg(border_color)),
    )
}

/// The dialog's input line. Drawn on the notice row, in the repo header's
/// place: the header names the repo being left, the input names the one being
/// opened, and only one of those is being decided right now. A path longer
/// than the row is shown from its tail behind a leading `…` — the caret marks
/// where typing lands, so it is the end that must survive. `width` is the
/// row's; 0 means "unknown", which keeps the whole path.
pub(crate) fn repo_input_line<'a>(
    repo_input: &'a RepoInput,
    accent: Color,
    width: u16,
) -> Line<'a> {
    const CARET: &str = "|";
    let accent_style = Style::default().fg(accent);
    let (prompt, value, caret) = match (&repo_input.mode, &repo_input.clone_status) {
        (RepoInputMode::CloneParent | RepoInputMode::CloneUrl, CloneUiStatus::Running) => {
            (" cloning... ", repo_input.clone_url.as_str(), false)
        }
        (RepoInputMode::CloneParent | RepoInputMode::CloneUrl, CloneUiStatus::Unconfirmed) => (
            " clone outcome unknown: ",
            repo_input.clone_url.as_str(),
            false,
        ),
        (RepoInputMode::CloneParent | RepoInputMode::CloneUrl, CloneUiStatus::Starting) => {
            (" starting clone... ", repo_input.clone_url.as_str(), false)
        }
        (RepoInputMode::CloneParent | RepoInputMode::CloneUrl, CloneUiStatus::Discovering) => {
            (" finding clone... ", repo_input.clone_url.as_str(), false)
        }
        (RepoInputMode::CloneParent | RepoInputMode::CloneUrl, CloneUiStatus::Done(path)) => {
            (" cloned: ", path.as_str(), false)
        }
        (RepoInputMode::Open, _) => (" repo: ", repo_input.buf.as_str(), true),
        (RepoInputMode::CloneParent, _) => (" clone in: ", repo_input.clone_parent.as_str(), true),
        (RepoInputMode::CloneUrl, _) => (" clone URL: ", repo_input.clone_url.as_str(), true),
    };
    let prompt_width = Span::raw(prompt).width();
    let caret_width = if caret { Span::raw(CARET).width() } else { 0 };
    let safe_value = crate::workspace::clean_clone_display(value);
    let path = visible_tail(
        &safe_value,
        (width as usize).saturating_sub(prompt_width + caret_width),
        width == 0,
    );
    let mut spans = vec![Span::styled(prompt, accent_style), Span::raw(path)];
    if caret {
        spans.push(Span::styled(CARET, accent_style));
    }
    Line::from(spans)
}

/// The end of `buf` as `budget` display columns hold it, behind a `…` when the
/// front had to go. Columns, not bytes: a path can hold wide characters.
fn visible_tail(buf: &str, budget: usize, unbounded: bool) -> String {
    if unbounded || Span::raw(buf).width() <= budget {
        return buf.to_string();
    }
    // A row too narrow for even the ellipsis gets nothing: the mark would
    // itself push the caret off the end it exists to protect.
    let ellipsis_width = Span::raw("…").width();
    if budget < ellipsis_width {
        return String::new();
    }
    let mut used = 0;
    let mut start = buf.len();
    for (i, ch) in buf.char_indices().rev() {
        let w = Span::raw(ch.to_string()).width();
        if used + w > budget - ellipsis_width {
            break;
        }
        used += w;
        start = i;
    }
    // Per-character sums miss context-sensitive sequences (a variation
    // selector, a combining mark), and here a column over costs the caret.
    // Re-measure the built tail once and shed from its front until it fits;
    // the loop runs only on the rare sequence where the sums disagree.
    while start < buf.len() && ellipsis_width + Span::raw(&buf[start..]).width() > budget {
        start += buf[start..].chars().next().map_or(1, char::len_utf8);
    }
    format!("…{}", &buf[start..])
}

/// The hint row while the dialog is open: the dialog's keys, unless a notice
/// (a rejected path) or the Tab candidates claim the row first. Same priority
/// the notice row applies when it is free, for the same reasons — a rejection
/// explains the enter that just did nothing, the candidates answer the Tab
/// that is on screen, and any edit clears both, so the legend is never gone
/// for long. No `repo_path` chip beside the notice: the path in question is
/// the one in the input directly above.
pub(crate) fn repo_dialog_hint_line<'a>(
    notice: Option<&'a Notice>,
    repo_input: &'a RepoInput,
    width: u16,
) -> Line<'a> {
    if repo_input.mode != RepoInputMode::Open
        && !notice.is_some_and(|notice| notice.kind == crate::app::NoticeKind::RepoInput)
        && matches!(
            repo_input.clone_status,
            CloneUiStatus::Discovering
                | CloneUiStatus::Starting
                | CloneUiStatus::Running
                | CloneUiStatus::Unconfirmed
                | CloneUiStatus::Done(_)
                | CloneUiStatus::Failed(_)
        )
    {
        return clone_dialog_hint(repo_input, width);
    }
    if let Some(line) = notice_or_candidates(notice, repo_input, None, width) {
        return line;
    }
    if repo_input.mode != RepoInputMode::Open {
        return clone_dialog_hint(repo_input, width);
    }
    let legend = if repo_input.picker.is_some() {
        " esc: back | up/dn/jk: move | right: open | left: up | enter: open | shift+tab: clone"
    } else {
        " down: browse | tab: complete | enter: open | shift+tab: clone | esc: cancel"
    };
    Line::from(Span::styled(legend, Style::default().fg(Color::DarkGray)))
}

fn clone_dialog_hint(repo_input: &RepoInput, width: u16) -> Line<'static> {
    let dark = Style::default().fg(Color::DarkGray);
    let error = Style::default()
        .fg(Color::Red)
        .add_modifier(ratatui::style::Modifier::BOLD);
    let text = match &repo_input.clone_status {
        CloneUiStatus::Starting => "Starting clone… Esc hides; the clone continues.".to_string(),
        CloneUiStatus::Discovering => "Checking for a running clone… Esc hides.".to_string(),
        CloneUiStatus::Running => "Clone running · Esc hides; the clone continues.".to_string(),
        CloneUiStatus::Unconfirmed => {
            if repo_input.mode == RepoInputMode::CloneParent {
                "Clone outcome unknown · check destination · Enter: URL".to_string()
            } else {
                "Clone outcome unknown · check destination · Enter retries".to_string()
            }
        }
        CloneUiStatus::Done(path) => {
            format!("Clone finished · Enter opens · Esc hides · {path}")
        }
        CloneUiStatus::Failed(message) => {
            if repo_input.mode == RepoInputMode::CloneParent {
                format!("Enter: URL · Esc cancel · {message}")
            } else {
                format!("Enter retry · Esc/Shift+Tab parent · {message}")
            }
        }
        CloneUiStatus::Idle if repo_input.mode == RepoInputMode::CloneParent => {
            if !repo_input.clone_available {
                "git unavailable · shift+tab: open · esc: cancel".to_string()
            } else if repo_input.picker.is_some() {
                "esc back · up/dn/jk move · right open · left up · enter select · shift+tab open"
                    .to_string()
            } else {
                "tab: complete · down: browse · enter: next · shift+tab: open · esc: cancel"
                    .to_string()
            }
        }
        CloneUiStatus::Idle if repo_input.mode == RepoInputMode::CloneUrl => {
            if !repo_input.clone_available {
                "git unavailable · Shift+Tab: parent · Esc: back".to_string()
            } else {
                let preview = crate::git::clone::validate_clone_url(&repo_input.clone_url)
                    .ok()
                    .map(|name| {
                        std::path::Path::new(&repo_input.clone_parent)
                            .join(name)
                            .to_string_lossy()
                            .into_owned()
                    });
                match preview {
                    Some(path) => {
                        let suffix = " · Enter clone · Shift+Tab parent · Esc back";
                        let budget = (width as usize)
                            .saturating_sub(ratatui::text::Span::raw("to: ").width())
                            .saturating_sub(ratatui::text::Span::raw(suffix).width());
                        format!(
                            "to: {}{suffix}",
                            visible_tail(
                                &crate::workspace::clean_clone_display(&path),
                                budget,
                                false
                            )
                        )
                    }
                    None => "Enter: clone · Shift+Tab: parent · Esc: back".to_string(),
                }
            }
        }
        CloneUiStatus::Idle => String::new(),
    };
    let style = if matches!(repo_input.clone_status, CloneUiStatus::Failed(_)) {
        error
    } else {
        dark
    };
    let fitted = fit_hint(&text, width as usize);
    Line::from(Span::styled(fitted, style))
}

fn fit_hint(text: &str, width: usize) -> String {
    let safe_text = crate::workspace::clean_clone_text(text);
    if ratatui::text::Span::raw(&safe_text).width() <= width {
        return safe_text;
    }
    let mut end = safe_text.len();
    while end > 0 && ratatui::text::Span::raw(&safe_text[..end]).width() > width {
        end -= safe_text[..end]
            .chars()
            .next_back()
            .map_or(1, char::len_utf8);
    }
    safe_text[..end].to_string()
}

#[cfg(test)]
#[path = "repo_dialog_tests.rs"]
mod tests;
