#[cfg(test)]
mod tests;

mod file_view;
mod gutter;
mod split_view;
mod title;

pub(crate) use file_view::render_file_view;
pub(crate) use split_view::render_split_view;

use crate::app::{App, DiffPaneView, Focus, ViewMode};
use crate::git::diff::LineKind;
use crate::ui::{focused_border_style, path_extension, render_search_bar};
use gutter::{lineno_digits, render_gutter_and_body, unified_gutter_text, unified_gutter_width};
use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Style},
    text::{Line, Span},
    widgets::{Block, Borders},
};
use syntect::highlighting::ThemeSet;
use syntect::parsing::SyntaxSet;
use title::unified_title;

/// Minimum pane width (columns) for the side-by-side split layout. Below this
/// each half is too narrow to read, so `Split` view falls back to the unified
/// renderer. Raised from 80 by both gutters to keep the readable code width
/// per side rather than silently shrinking it.
const MIN_SPLIT_WIDTH: u16 = 90;

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

    // Build the syntect highlight cache once per (hunks × per-hunk syntax) so
    // the visible-window walk stays bounded even on large diffs.
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

    // Gutter width is a property of the whole loaded diff, not of the visible
    // window, so the body's left edge stays put while scrolling. With no diff
    // loaded the pane holds only a placeholder message, which has no line to
    // number.
    let digits = lineno_digits(&app.diff.hunks);
    let gutter_width = if total_lines == 0 {
        0
    } else {
        unified_gutter_width(digits)
    };

    let mut lines: Vec<Line> = Vec::with_capacity(visible_height);
    // Collected in lockstep with `lines`: same rows, same order, so the two
    // paragraphs share one vertical window.
    let mut gutter_lines: Vec<Line> = Vec::with_capacity(visible_height);
    let mut flat_idx: usize = 0;

    'outer: for (hi, hunk) in app.diff.hunks.iter().enumerate() {
        if flat_idx >= visible_end {
            break;
        }

        if flat_idx >= scroll_start && flat_idx < visible_end {
            lines.push(Line::from(Span::styled(
                hunk.header.as_str(),
                Style::default().fg(Color::Cyan),
            )));
            // Blank, but full width: a header row with no gutter cell would
            // start its `@@` one column left of the body's left edge.
            gutter_lines.push(Line::from(""));
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

            // Read from the prebuilt highlight cache; the shape is guaranteed
            // to match `hunks` after `ensure_highlight_cache`, so a mismatch
            // only hits the fallback that renders the raw text.
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
            // Same background as the row so the number reads as part of it
            // rather than as a column floating beside the highlight.
            gutter_lines.push(Line::from(Span::styled(
                unified_gutter_text(diff_line.old_lineno, diff_line.new_lineno, digits),
                Style::default().fg(Color::DarkGray).bg(bg),
            )));
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
            ViewMode::Tree => {
                // Tree mode renders the file overlay, not the unified diff, so
                // this message is only reachable when the diff view is forced
                // open with no file selected.
                "Select a file to preview"
            }
        };
        lines.push(Line::from(Span::styled(
            msg,
            Style::default().fg(Color::DarkGray),
        )));
    }

    // The block is rendered on its own rather than attached to a paragraph:
    // the gutter and the body are two paragraphs sharing one bordered area.
    let block = Block::default()
        .borders(Borders::ALL)
        .title(unified_title(app))
        .border_style(border_style);
    let inner = block.inner(diff_area);
    frame.render_widget(block, diff_area);
    render_gutter_and_body(
        frame,
        inner,
        gutter_width,
        gutter_lines,
        lines,
        app.diff.scroll_x.min(u16::MAX as usize) as u16,
        app.diff.wrap,
    );

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
