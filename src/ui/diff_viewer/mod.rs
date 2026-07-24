mod file_view;
mod split_view;

pub(crate) use file_view::render_file_view;
pub(crate) use split_view::render_split_view;

use crate::app::{App, DiffPaneView, Focus, ViewMode};
use crate::git::diff::LineKind;
use crate::ui::{focused_border_style, jump_legend, path_extension, render_search_bar};
use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph},
};
use syntect::highlighting::ThemeSet;
use syntect::parsing::SyntaxSet;

/// Minimum pane width (columns) for the side-by-side split layout. Below this
/// each half is too narrow to read, so `Split` view transparently falls back
/// to the unified diff renderer.
const MIN_SPLIT_WIDTH: u16 = 80;

pub(crate) fn rgb_to_color(rgb: (u8, u8, u8)) -> Color {
    Color::Rgb(rgb.0, rgb.1, rgb.2)
}

pub fn render(
    frame: &mut Frame,
    app: &mut App,
    area: Rect,
    ss: &SyntaxSet,
    ts: &ThemeSet,
    accent: ratatui::style::Color,
) {
    if app.diff.view == DiffPaneView::File {
        render_file_view(frame, app, area, ss, ts, accent);
        return;
    }

    // Render side-by-side only when there is a diff to split and the pane is
    // wide enough; otherwise fall through to the unified renderer below.
    if app.diff.view == DiffPaneView::Split
        && area.width >= MIN_SPLIT_WIDTH
        && !app.diff.hunks.is_empty()
    {
        render_split_view(frame, app, area, ss, ts, accent);
        return;
    }

    let show_search = app.diff.search.is_visible();

    let (diff_area, search_area) = if show_search {
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Min(0), Constraint::Length(1)])
            .split(area);
        (chunks[0], Some(chunks[1]))
    } else {
        (area, None)
    };

    let focused = app.focus == Focus::DiffViewer;
    let border_style = focused_border_style(focused, accent);

    // Build the syntect highlight cache once per (hunks × per-hunk syntax)
    // so the visible-window walk below stays bounded even on large diffs.
    // Each hunk carries its own file_path now, so commit diffs that touch
    // multiple file types stop rendering as plain text.
    app.diff.ensure_highlight_cache(ss, ts);

    let current_match = app.diff.search.current_match();
    let has_search = app.diff.search.has_query();

    // Total flat row count = (1 hunk header + N body lines) per hunk.
    let total_lines = app.diff.line_count();
    let visible_height = (diff_area.height as usize).saturating_sub(2);
    let scroll_start = app.diff.scroll.min(app.diff.max_scroll());
    // Keep the stored cursor in sync with the clamped value so a Split-view
    // scroll position that overshoots this (narrower) unified fallback layout
    // is corrected on the frame it falls back.
    app.diff.scroll = scroll_start;
    let visible_end = scroll_start.saturating_add(visible_height);

    let mut lines: Vec<Line> = Vec::with_capacity(visible_height);
    let mut flat_idx: usize = 0;

    'outer: for (hi, hunk) in app.diff.hunks.iter().enumerate() {
        if flat_idx >= visible_end {
            break;
        }

        // Hunk header
        if flat_idx >= scroll_start && flat_idx < visible_end {
            lines.push(Line::from(Span::styled(
                hunk.header.as_str(),
                Style::default().fg(Color::Cyan),
            )));
        }
        flat_idx += 1;

        for (li, diff_line) in hunk.lines.iter().enumerate() {
            if flat_idx >= visible_end {
                break 'outer;
            }
            if flat_idx < scroll_start {
                flat_idx += 1;
                continue;
            }

            let is_current = has_search && current_match == Some(flat_idx);
            let is_match = has_search && app.diff.search.is_match(flat_idx);

            let bg = if is_current {
                Color::Rgb(100, 80, 0)
            } else if is_match {
                Color::Rgb(50, 42, 0)
            } else {
                match diff_line.kind {
                    LineKind::Added => Color::Rgb(0, 50, 0),
                    LineKind::Removed => Color::Rgb(50, 0, 0),
                    LineKind::Context => Color::Reset,
                }
            };

            let prefix = match diff_line.kind {
                LineKind::Added => "+",
                LineKind::Removed => "-",
                LineKind::Context => " ",
            };

            let mut spans = vec![Span::styled(
                prefix,
                Style::default().fg(Color::DarkGray).bg(bg),
            )];

            // Read from the prebuilt highlight cache. Shape is guaranteed to
            // match `hunks` after `ensure_highlight_cache`; treat any
            // mismatch as a fallback path that just renders the raw text.
            if let Some(segs) = app.diff.line_highlights.get(hi).and_then(|hh| hh.get(li)) {
                for seg in segs {
                    spans.push(Span::styled(
                        seg.text.as_str(),
                        Style::default().fg(rgb_to_color(seg.rgb)).bg(bg),
                    ));
                }
            } else {
                spans.push(Span::styled(
                    diff_line.content.as_str(),
                    Style::default().bg(bg),
                ));
            }

            lines.push(Line::from(spans));
            flat_idx += 1;
        }
    }

    if lines.is_empty() && total_lines == 0 {
        let msg = match app.mode {
            ViewMode::Log => {
                if app.log_view.commits.is_empty() {
                    "No commits in repository"
                } else {
                    "No diff for selected commit"
                }
            }
            ViewMode::Status => {
                if app.status_view.files.is_empty() {
                    "No changes in repository"
                } else {
                    "No diff for selected file"
                }
            }
            // Tree mode renders the file overlay, not the unified diff, so this
            // message is only reachable if the diff view is forced open with no
            // file selected.
            ViewMode::Tree => "Select a file to preview",
        };
        lines.push(Line::from(Span::styled(
            msg,
            Style::default().fg(Color::DarkGray),
        )));
    }

    let jump = jump_legend(app, '2');
    let title = match app.mode {
        ViewMode::Log => {
            let label = if app.log_view.diff_title.is_empty() {
                "Diff"
            } else {
                app.log_view.diff_title.as_str()
            };
            if has_search {
                let count = app.diff.search.matches.len();
                if count == 0 {
                    format!(" {jump} {label} [no matches] ")
                } else {
                    format!(
                        " {jump} {label} [{}/{}] ",
                        app.diff.search.cursor + 1,
                        count
                    )
                }
            } else {
                format!(" {jump} {label} ")
            }
        }
        ViewMode::Status => {
            let selected = app.selected_filtered_status_file();
            if has_search {
                let count = app.diff.search.matches.len();
                let file = selected.map(|f| f.path.as_str()).unwrap_or("Diff");
                if count == 0 {
                    format!(" {jump} {file} [no matches] ")
                } else {
                    format!(" {jump} {file} [{}/{}] ", app.diff.search.cursor + 1, count)
                }
            } else if let Some(f) = selected {
                format!(" {jump} {} ", f.path)
            } else {
                format!(" {jump} Diff ")
            }
        }
        ViewMode::Tree => {
            let path = app.tree_view.selected_path();
            let label = path.as_deref().unwrap_or("File");
            if has_search {
                let count = app.diff.search.matches.len();
                if count == 0 {
                    format!(" {jump} {label} [no matches] ")
                } else {
                    format!(
                        " {jump} {label} [{}/{}] ",
                        app.diff.search.cursor + 1,
                        count
                    )
                }
            } else {
                format!(" {jump} {label} ")
            }
        }
    };

    let para = Paragraph::new(lines)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title(title)
                .border_style(border_style),
        )
        .scroll((0, app.diff.scroll_x.min(u16::MAX as usize) as u16));

    frame.render_widget(para, diff_area);

    if let Some(sa) = search_area {
        render_search_bar(
            frame,
            app.diff.search.query.as_str(),
            app.diff.search.active,
            sa,
            accent,
        );
    }
}
