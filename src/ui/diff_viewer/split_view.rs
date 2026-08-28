use crate::app::{App, Focus};
use crate::git::diff::LineKind;
use crate::ui::diff_pane::SplitRow;
use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Style},
    text::{Line, Span},
    widgets::{Block, Borders},
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
    // Pin the shared scroll cursor to what this layout can actually show: the
    // split layout is shorter than the unified flat-row count (paired changes
    // collapse onto one row), and navigation clamps against the unified max.
    app.diff.scroll = scroll_start;
    let scroll_end = scroll_start.saturating_add(visible_height).min(rows.len());

    // Each half carries the number of the side it shows: old on the left, new
    // on the right. Collected in lockstep with the body lines so the two
    // paragraphs of a half share one vertical window.
    let digits = super::gutter::lineno_digits(&app.diff.hunks);
    let gutter_width = super::gutter::side_gutter_width(digits);

    let mut left_lines: Vec<Line> = Vec::with_capacity(visible_height);
    let mut right_lines: Vec<Line> = Vec::with_capacity(visible_height);
    let mut left_gutter: Vec<Line> = Vec::with_capacity(visible_height);
    let mut right_gutter: Vec<Line> = Vec::with_capacity(visible_height);
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
                // Blank but present: a header with no gutter cell would start
                // one column left of the body beneath it.
                left_gutter.push(Line::from(""));
                right_gutter.push(Line::from(""));
            }
            SplitRow::Body { left, right } => {
                let (lg, lb) = split_side_lines(app, *left, digits, Side::Old);
                let (rg, rb) = split_side_lines(app, *right, digits, Side::New);
                left_lines.push(lb);
                right_lines.push(rb);
                left_gutter.push(lg);
                right_gutter.push(rg);
            }
        }
    }

    let title = super::title::split_title(app);
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
    super::gutter::render_gutter_and_body(
        frame,
        halves[0],
        gutter_width,
        left_gutter,
        left_lines,
        scroll_x,
        // Wrapping is deliberately ignored here: halves that fold to
        // different heights stop lining up, and lining up is the point.
        false,
    );

    // A left border on the right column draws the vertical divider between the
    // two halves and indents the new-side content by one cell. It is rendered
    // on its own so the gutter and body can split the area inside it.
    let right_block = Block::default()
        .borders(Borders::LEFT)
        .border_style(border_style);
    let right_inner = right_block.inner(halves[1]);
    frame.render_widget(right_block, halves[1]);
    super::gutter::render_gutter_and_body(
        frame,
        right_inner,
        gutter_width,
        right_gutter,
        right_lines,
        scroll_x,
        false,
    );
}

/// Which side's line number a half shows.
enum Side {
    Old,
    New,
}

/// Build one side's gutter and body `Line` for a split body row, as
/// `(gutter, body)`. `None` (no counterpart line on this side) renders both as
/// blank; otherwise the cell is styled by line kind and reuses the prebuilt
/// highlight cache, mirroring the unified renderer. Both lines come from one
/// lookup so they cannot disagree about which `DiffLine` the row is showing.
fn split_side_lines<'a>(
    app: &'a App,
    cell: Option<(usize, usize)>,
    digits: usize,
    side: Side,
) -> (Line<'a>, Line<'a>) {
    let blank = || (Line::from(""), Line::from(""));
    let Some((hi, li)) = cell else {
        return blank();
    };
    let Some(diff_line) = app.diff.hunks.get(hi).and_then(|h| h.lines.get(li)) else {
        return blank();
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

    let lineno = match side {
        Side::Old => diff_line.old_lineno,
        Side::New => diff_line.new_lineno,
    };
    let gutter = Line::from(Span::styled(
        super::gutter::side_gutter_text(lineno, digits),
        Style::default().fg(Color::DarkGray).bg(bg),
    ));
    (gutter, Line::from(spans))
}
