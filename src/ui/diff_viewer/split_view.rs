use crate::app::{App, Focus, ViewMode};
use crate::git::diff::LineKind;
use crate::ui::diff_pane::SplitRow;
use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph},
};
use syntect::highlighting::ThemeSet;
use syntect::parsing::SyntaxSet;

pub(crate) fn render_split_view(
    frame: &mut Frame,
    app: &mut App,
    area: Rect,
    ss: &SyntaxSet,
    ts: &ThemeSet,
    accent: ratatui::style::Color,
) {
    let focused = app.focus == Focus::DiffViewer;
    let border_style = super::focused_border_style(focused, accent);
    app.diff.ensure_highlight_cache(ss, ts);

    let rows = app.diff.split_rows();
    let visible_height = (area.height as usize).saturating_sub(2);
    let max_scroll = rows.len().saturating_sub(1);
    let scroll_start = app.diff.scroll.min(max_scroll);
    // Pin the shared scroll cursor to what this layout can actually show. The
    // split layout is shorter than the unified flat-row count (paired changes
    // collapse onto one row), and navigation clamps against the unified max —
    // writing the clamped value back keeps `k`/pgup responsive immediately
    // after bottoming out instead of unwinding phantom rows.
    app.diff.scroll = scroll_start;
    let scroll_end = scroll_start.saturating_add(visible_height).min(rows.len());

    let mut left_lines: Vec<Line> = Vec::with_capacity(visible_height);
    let mut right_lines: Vec<Line> = Vec::with_capacity(visible_height);
    for row in &rows[scroll_start..scroll_end] {
        match row {
            SplitRow::Header(hi) => {
                let header = app
                    .diff
                    .hunks
                    .get(*hi)
                    .map(|h| h.header.as_str())
                    .unwrap_or("");
                left_lines.push(Line::from(Span::styled(
                    header,
                    Style::default().fg(Color::Cyan),
                )));
                right_lines.push(Line::from(""));
            }
            SplitRow::Body { left, right } => {
                left_lines.push(split_side_line(app, *left));
                right_lines.push(split_side_line(app, *right));
            }
        }
    }

    let title = split_title(app);
    let block = Block::default()
        .borders(Borders::ALL)
        .title(title)
        .border_style(border_style);
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let halves = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
        .split(inner);

    let scroll_x = app.diff.scroll_x.min(u16::MAX as usize) as u16;
    let left_para = Paragraph::new(left_lines).scroll((0, scroll_x));
    // A left border on the right column draws the vertical divider between
    // the two halves and indents the new-side content by one cell.
    let right_para = Paragraph::new(right_lines)
        .block(
            Block::default()
                .borders(Borders::LEFT)
                .border_style(border_style),
        )
        .scroll((0, scroll_x));

    frame.render_widget(left_para, halves[0]);
    frame.render_widget(right_para, halves[1]);
}

/// Build one side's `Line` for a split body row. `None` (no counterpart line
/// on this side) renders as a blank line; otherwise the cell is styled by line
/// kind and reuses the prebuilt highlight cache, mirroring the unified
/// renderer's per-line treatment.
fn split_side_line<'a>(app: &'a App, cell: Option<(usize, usize)>) -> Line<'a> {
    let Some((hi, li)) = cell else {
        return Line::from("");
    };
    let Some(diff_line) = app.diff.hunks.get(hi).and_then(|h| h.lines.get(li)) else {
        return Line::from("");
    };

    let bg = match diff_line.kind {
        LineKind::Added => Color::Rgb(0, 50, 0),
        LineKind::Removed => Color::Rgb(50, 0, 0),
        LineKind::Context => Color::Reset,
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
    if let Some(segs) = app.diff.line_highlights.get(hi).and_then(|hh| hh.get(li)) {
        for seg in segs {
            spans.push(Span::styled(
                seg.text.as_str(),
                Style::default().fg(super::rgb_to_color(seg.rgb)).bg(bg),
            ));
        }
    } else {
        spans.push(Span::styled(
            diff_line.content.as_str(),
            Style::default().bg(bg),
        ));
    }
    Line::from(spans)
}

/// Title for the split pane: the same file/commit label the unified view uses,
/// tagged `[split]`. Search match counts are omitted because the split view
/// does not render search highlights.
fn split_title(app: &App) -> String {
    let label = match app.mode {
        ViewMode::Log => {
            if app.log_view.diff_title.is_empty() {
                "Diff".to_string()
            } else {
                app.log_view.diff_title.clone()
            }
        }
        ViewMode::Status => app
            .selected_filtered_status_file()
            .map(|f| f.path.clone())
            .unwrap_or_else(|| "Diff".to_string()),
        ViewMode::Tree => app
            .tree_view
            .selected_path()
            .unwrap_or_else(|| "File".to_string()),
    };
    format!(" {} {label} [split] ", super::jump_legend(app, '2'))
}
