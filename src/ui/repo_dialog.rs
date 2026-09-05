//! The repo dialog's two chrome rows: the input line on the notice row, its
//! keys and reports on the hint row.

use crate::app::Notice;
use crate::ui::notice::notice_or_candidates;
use crate::ui::status_view::RepoInput;
use ratatui::{
    style::{Color, Style},
    text::{Line, Span},
};

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
    const PROMPT: &str = " repo: ";
    const CARET: &str = "|";
    let accent_style = Style::default().fg(accent);
    let path = visible_tail(
        &repo_input.buf,
        (width as usize).saturating_sub(Span::raw(PROMPT).width() + Span::raw(CARET).width()),
        width == 0,
    );
    Line::from(vec![
        Span::styled(PROMPT, accent_style),
        path,
        Span::styled(CARET, accent_style),
    ])
}

/// The end of `buf` as `budget` display columns hold it, behind a `…` when the
/// front had to go. Columns, not bytes: a path can hold wide characters.
fn visible_tail(buf: &str, budget: usize, unbounded: bool) -> Span<'_> {
    if unbounded || Span::raw(buf).width() <= budget {
        return Span::raw(buf);
    }
    // A row too narrow for even the ellipsis gets nothing: the mark would
    // itself push the caret off the end it exists to protect.
    let ellipsis_width = Span::raw("…").width();
    if budget < ellipsis_width {
        return Span::raw("");
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
    Span::raw(format!("…{}", &buf[start..]))
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
    if let Some(line) = notice_or_candidates(notice, repo_input, None, width) {
        return line;
    }
    let legend = if repo_input.picker.is_some() {
        " up/dn/jk: move | right: open | left: up | enter: open | esc: back"
    } else {
        " down: browse | tab: complete | enter: open | esc: cancel"
    };
    Line::from(Span::styled(legend, Style::default().fg(Color::DarkGray)))
}
