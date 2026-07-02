use crate::app::{App, Focus};
use crate::backend::PaneId;
use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Position, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph},
};

/// The terminal pane draws only top/bottom borders, never the left/right `│`.
/// With side bars, selecting terminal output to copy picks up a `│` glyph on
/// every wrapped row; dropping them lets the content run edge-to-edge so a
/// copy is clean. Top stays for the title + focus tint, bottom for separation.
const TERMINAL_BORDERS: Borders = Borders::TOP.union(Borders::BOTTOM);

/// Per-tab character budget for the title (excluding the `F#` key hint and
/// surrounding padding). Anything longer is truncated with a trailing ellipsis
/// so long OSC-set titles can't push neighboring tabs off the row.
const TAB_TITLE_MAX_CHARS: usize = 20;

/// Truncate `title` to at most `max` characters, appending `…` when cut.
/// Char-based (not display-width) for simplicity: ASCII shell program names
/// are the common case and `chars().count()` is already correct there. CJK
/// titles render slightly under the visual budget, which is acceptable.
fn truncate_tab_title(title: &str, max: usize) -> String {
    if title.chars().count() <= max {
        return title.to_string();
    }
    // Reserve one char of the budget for the ellipsis itself.
    let keep = max.saturating_sub(1);
    let mut out: String = title.chars().take(keep).collect();
    out.push('…');
    out
}

pub(crate) fn content_area(area: Rect) -> Option<Rect> {
    terminal_layout(area).map(|(_, content)| content)
}

fn terminal_layout(area: Rect) -> Option<(Rect, Rect)> {
    let inner = Block::default().borders(TERMINAL_BORDERS).inner(area);
    if inner.height == 0 || inner.width == 0 {
        return None;
    }
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(1), Constraint::Min(0)])
        .split(inner);
    Some((chunks[0], chunks[1]))
}

/// Compute the visible pane-index window `[start, start+len)` for a split
/// grid capped at `max_visible` panes. `prev_start` is the previous window's
/// start (0 for a fresh terminal); the window is nudged the minimum amount
/// needed to keep `active` inside it, rather than re-centering every call —
/// so paging through panes one at a time doesn't reshuffle the whole grid.
pub(crate) fn visible_range(
    prev_start: usize,
    active: usize,
    pane_count: usize,
    max_visible: usize,
) -> std::ops::Range<usize> {
    if pane_count == 0 || max_visible == 0 {
        return 0..0;
    }
    let window = max_visible.min(pane_count);
    let active = active.min(pane_count - 1);
    let max_start = pane_count - window;

    let mut start = prev_start.min(max_start);
    if active < start {
        start = active;
    } else if active >= start + window {
        start = active + 1 - window;
    }
    start..(start + window)
}

/// Split `area` into `count` cells using a balanced grid: 1 pane fills the
/// area; 2 panes go side by side when `area` is wide, stacked otherwise; 3
/// panes get a 2-column row plus a full-width remainder row; 4 is a 2x2
/// grid; 5-6 use 3 columns; 7 uses a 4-then-3 row split. Counts beyond that
/// (not expected given `MAX_VISIBLE_FULLSCREEN`) fall back to a near-square
/// grid. Every returned Rect has at least 1x1 size when `area` is at least
/// `count` cells large, so no cell silently disappears.
pub(crate) fn split_pane_areas(area: Rect, count: usize) -> Vec<Rect> {
    if count == 0 || area.width == 0 || area.height == 0 {
        return Vec::new();
    }
    let plan = grid_row_plan(count, area);
    split_by_row_plan(area, &plan)
}

/// One entry per row, each entry the number of columns in that row.
fn grid_row_plan(count: usize, area: Rect) -> Vec<usize> {
    match count {
        1 => vec![1],
        2 => {
            if area.width >= area.height.saturating_mul(2) {
                vec![2]
            } else {
                vec![1, 1]
            }
        }
        3 => vec![2, 1],
        4 => vec![2, 2],
        5 => vec![3, 2],
        6 => vec![3, 3],
        7 => vec![4, 3],
        n => {
            let cols = (n as f64).sqrt().ceil() as usize;
            let rows = n.div_ceil(cols);
            let mut plan = vec![cols; rows];
            let mut excess = cols * rows - n;
            let mut i = plan.len();
            while excess > 0 && i > 0 {
                i -= 1;
                let take = plan[i].saturating_sub(1).min(excess);
                plan[i] -= take;
                excess -= take;
            }
            plan.retain(|&c| c > 0);
            plan
        }
    }
}

fn split_by_row_plan(area: Rect, plan: &[usize]) -> Vec<Rect> {
    if plan.is_empty() {
        return Vec::new();
    }
    let row_constraints: Vec<Constraint> = plan.iter().map(|_| Constraint::Min(1)).collect();
    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints(row_constraints)
        .split(area);

    let mut result = Vec::with_capacity(plan.iter().sum());
    for (row_area, &cols) in rows.iter().zip(plan.iter()) {
        if cols == 0 {
            continue;
        }
        let col_constraints: Vec<Constraint> = (0..cols).map(|_| Constraint::Min(1)).collect();
        let cells = Layout::default()
            .direction(Direction::Horizontal)
            .constraints(col_constraints)
            .split(*row_area);
        result.extend(cells.iter().copied());
    }
    result
}

pub fn render(frame: &mut Frame, app: &App, area: Rect, accent: Color) {
    let focused = app.focus == Focus::Terminal;
    let border_style = super::focused_border_style(focused, accent);

    let label = if app.terminal.is_scrolled() {
        " Terminal [SCROLL — shift+pgdn: down | input: live] "
    } else {
        " Terminal "
    };
    // The upper panes draw a `┌` corner that pushes their title text in by one
    // column (`┌ F1 Files`). This pane has no left border, so a border-styled
    // `─` stands in for that corner — it keeps `Terminal` column-aligned with
    // `F1 Files` / `F2 Diff` above and makes the line start flush at the edge.
    let title = Line::from(vec![Span::styled("─", border_style), Span::raw(label)]);
    let block = Block::default()
        .borders(TERMINAL_BORDERS)
        .title(title)
        .border_style(border_style);

    frame.render_widget(block, area);

    let Some((tab_area, content_area)) = terminal_layout(area) else {
        return;
    };

    let pane_count = app.terminal.panes.len();
    let visible = visible_range(
        app.terminal.visible_start,
        app.terminal.active,
        pane_count,
        app.terminal.max_visible(),
    );

    render_tab_bar(frame, app, tab_area, accent, visible.clone());

    if visible.is_empty() {
        let screen_lines = vec![Line::from(Span::styled(
            format!(" No terminal — press {} t to open one ", app.leader_label()),
            Style::default().fg(Color::DarkGray),
        ))];
        frame.render_widget(Paragraph::new(screen_lines), content_area);
        return;
    }

    let visible_ids: Vec<PaneId> = app.terminal.panes[visible.clone()]
        .iter()
        .map(|p| p.id)
        .collect();

    if visible_ids.len() == 1 {
        // The common case — one pane fills the whole area exactly as before
        // split view existed: no per-cell border, content runs edge-to-edge
        // so copying terminal output never picks up a stray `│`.
        let id = visible_ids[0];
        let screen_lines = build_screen_lines(app, id, content_area.height, content_area.width);
        frame.render_widget(Paragraph::new(screen_lines), content_area);
        render_cursor(frame, app, id, content_area);
        return;
    }

    let cells = split_pane_areas(content_area, visible_ids.len());
    for (offset, (&id, &cell)) in visible_ids.iter().zip(cells.iter()).enumerate() {
        let i = visible.start + offset;
        let is_active = i == app.terminal.active;
        let pane_border_style = if is_active {
            Style::default().fg(accent)
        } else {
            Style::default().fg(Color::DarkGray)
        };
        let pane_title = app
            .terminal
            .panes
            .get(i)
            .map(|p| truncate_tab_title(&p.title, TAB_TITLE_MAX_CHARS))
            .unwrap_or_default();
        let cell_block = Block::default()
            .borders(Borders::ALL)
            .border_style(pane_border_style)
            .title(format!(" {pane_title} "));
        let inner = cell_block.inner(cell);
        frame.render_widget(cell_block, cell);
        if inner.width == 0 || inner.height == 0 {
            continue;
        }
        let screen_lines = build_screen_lines(app, id, inner.height, inner.width);
        frame.render_widget(Paragraph::new(screen_lines), inner);
        if is_active {
            render_cursor(frame, app, id, inner);
        }
    }
}

fn render_tab_bar(
    frame: &mut Frame,
    app: &App,
    tab_area: Rect,
    accent: Color,
    visible: std::ops::Range<usize>,
) {
    let tab_spans: Vec<Span> = if app.terminal.panes.is_empty() {
        vec![Span::styled(
            format!(" {} t: new terminal ", app.leader_label()),
            Style::default().fg(Color::DarkGray),
        )]
    } else {
        let hidden_before = visible.start;
        let hidden_after = app.terminal.panes.len().saturating_sub(visible.end);
        let mut spans = Vec::new();
        if hidden_before > 0 {
            spans.push(Span::styled(
                format!(" +{hidden_before} "),
                Style::default().fg(Color::DarkGray),
            ));
        }
        spans.extend(app.terminal.panes[visible.clone()].iter().enumerate().map(
            |(offset, pane)| {
                let i = visible.start + offset;
                let style = if i == app.terminal.active {
                    Style::default()
                        .fg(Color::Black)
                        .bg(accent)
                        .add_modifier(Modifier::BOLD)
                } else {
                    Style::default().fg(Color::Gray)
                };
                // F3..=F9 (and the matching `<prefix> 3..9` digits) are wired
                // to panes 0..=6 in `input`; show the binding so the tab bar
                // doubles as a key legend. Panes past the 7th have no jump key,
                // so they carry no hint to avoid implying an unbound shortcut.
                let title = truncate_tab_title(&pane.title, TAB_TITLE_MAX_CHARS);
                let label = if i < 7 {
                    format!(" F{} {} ", i + 3, title)
                } else {
                    format!(" {} ", title)
                };
                Span::styled(label, style)
            },
        ));
        if hidden_after > 0 {
            spans.push(Span::styled(
                format!(" +{hidden_after} "),
                Style::default().fg(Color::DarkGray),
            ));
        }
        spans
    };
    frame.render_widget(Paragraph::new(Line::from(tab_spans)), tab_area);
}

fn build_screen_lines(app: &App, pane_id: PaneId, rows: u16, cols: u16) -> Vec<Line<'static>> {
    let Some(screen) = app.terminal.screen_for_pane(pane_id) else {
        return vec![Line::from(Span::styled(
            " (no output) ",
            Style::default().fg(Color::DarkGray),
        ))];
    };

    let (screen_rows, screen_cols) = screen.size();
    let render_rows = rows.min(screen_rows);
    let render_cols = cols.min(screen_cols);

    (0..render_rows)
        .map(|row| {
            let mut spans: Vec<Span<'static>> = Vec::new();
            let mut run_text = String::new();
            let mut run_style = Style::default();

            for col in 0..render_cols {
                let (text, style): (&str, Style) = match screen.cell(row, col) {
                    Some(cell) => {
                        // Wide chars (e.g., Hangul) occupy two columns: vt100
                        // stores the glyph on the first cell and an empty
                        // continuation on the second. Emitting anything for
                        // the continuation would shift the row by one column.
                        if cell.is_wide_continuation() {
                            continue;
                        }
                        let contents = cell.contents();
                        let t = if contents.is_empty() { " " } else { contents };
                        (t, cell_to_style(cell))
                    }
                    None => (" ", Style::default()),
                };

                if style != run_style {
                    if !run_text.is_empty() {
                        spans.push(Span::styled(std::mem::take(&mut run_text), run_style));
                    }
                    run_style = style;
                }
                run_text.push_str(text);
            }
            if !run_text.is_empty() {
                spans.push(Span::styled(run_text, run_style));
            }
            Line::from(spans)
        })
        .collect()
}

fn render_cursor(frame: &mut Frame, app: &App, pane_id: PaneId, area: Rect) {
    if app.focus != Focus::Terminal {
        return;
    }
    if app.terminal.is_scrolled() {
        return;
    }

    let Some(screen) = app.terminal.screen_for_pane(pane_id) else {
        return;
    };
    let Some(position) = screen_cursor_position(screen, area) else {
        return;
    };

    frame.set_cursor_position(position);
}

fn screen_cursor_position(screen: &vt100::Screen, area: Rect) -> Option<Position> {
    if area.height == 0 || area.width == 0 {
        return None;
    }

    // Embedded CLIs such as Claude can leave DECTCEM hide-cursor mode enabled
    // while still expecting an outer terminal host to expose the input point.
    // For the focused terminal pane, keep the host cursor visible at vt100's
    // tracked cursor position instead of honoring the inner app's hide flag.
    let (row, col) = screen.cursor_position();
    Some(Position::new(
        area.x.saturating_add(col.min(area.width.saturating_sub(1))),
        area.y
            .saturating_add(row.min(area.height.saturating_sub(1))),
    ))
}

fn cell_to_style(cell: &vt100::Cell) -> Style {
    let mut style = Style::default()
        .fg(vt100_color(cell.fgcolor()))
        .bg(vt100_color(cell.bgcolor()));
    if cell.bold() {
        style = style.add_modifier(Modifier::BOLD);
    }
    if cell.italic() {
        style = style.add_modifier(Modifier::ITALIC);
    }
    if cell.underline() {
        style = style.add_modifier(Modifier::UNDERLINED);
    }
    // Reverse video is how vim visual mode, fzf's cursor, and less's search
    // hit mark selections. Without it those selections render as plain text.
    if cell.inverse() {
        style = style.add_modifier(Modifier::REVERSED);
    }
    style
}

fn vt100_color(c: vt100::Color) -> Color {
    match c {
        vt100::Color::Default => Color::Reset,
        vt100::Color::Idx(i) => Color::Indexed(i),
        vt100::Color::Rgb(r, g, b) => Color::Rgb(r, g, b),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::tests::app_with_files;
    use ratatui::{Terminal, backend::TestBackend};

    #[test]
    fn maps_screen_cursor_to_render_area() {
        let mut parser = vt100::Parser::new(3, 10, 0);
        parser.process(b"\x1b[2;4H");

        let position = screen_cursor_position(parser.screen(), Rect::new(20, 10, 10, 3)).unwrap();

        assert_eq!(position, Position::new(23, 11));
    }

    #[test]
    fn short_title_passes_through_untouched() {
        assert_eq!(truncate_tab_title("claude", 24), "claude");
    }

    #[test]
    fn long_title_is_cut_with_ellipsis_within_budget() {
        let truncated = truncate_tab_title("claude-code: very-long-project-name", 24);
        assert_eq!(truncated.chars().count(), 24);
        assert!(truncated.ends_with('…'));
        assert!(truncated.starts_with("claude-code"));
    }

    #[test]
    fn title_exactly_at_budget_is_not_truncated() {
        let s: String = "a".repeat(24);
        assert_eq!(truncate_tab_title(&s, 24), s);
    }

    #[test]
    fn keeps_cursor_visible_when_terminal_requests_hide() {
        let mut parser = vt100::Parser::new(3, 10, 0);
        parser.process(b"\x1b[?25l\x1b[2;4H");

        let position = screen_cursor_position(parser.screen(), Rect::new(20, 10, 10, 3)).unwrap();

        assert_eq!(position, Position::new(23, 11));
    }

    #[test]
    fn content_spans_full_width_without_side_borders() {
        // The terminal content must reach both pane edges so copied output never
        // includes a `│`. Side borders would inset x by 1 and shrink width by 2.
        let area = Rect::new(0, 0, 40, 10);
        let content = content_area(area).unwrap();

        assert_eq!(content.x, area.x);
        assert_eq!(content.width, area.width);
    }

    #[test]
    fn render_does_not_resize_terminal_state() {
        let mut app = app_with_files(vec!["a.rs"]);
        app.terminal.size = (3, 10);
        let mut terminal = Terminal::new(TestBackend::new(40, 10)).unwrap();

        terminal
            .draw(|frame| {
                render(frame, &app, frame.area(), Color::Yellow);
            })
            .unwrap();

        assert_eq!(app.terminal.size, (3, 10));
    }

    #[test]
    fn visible_range_shows_everything_under_the_cap() {
        assert_eq!(visible_range(0, 0, 3, 4), 0..3);
    }

    #[test]
    fn visible_range_keeps_active_inside_a_capped_window() {
        // 7 panes, window of 4, active is the last pane: window must end at 7.
        assert_eq!(visible_range(0, 6, 7, 4), 3..7);
    }

    #[test]
    fn visible_range_moves_start_forward_only_as_far_as_needed() {
        // Previously showing [2,6). Active moves to 6 (just past the window):
        // start should shift by exactly 1, not jump to re-center.
        assert_eq!(visible_range(2, 6, 7, 4), 3..7);
    }

    #[test]
    fn visible_range_moves_start_backward_when_active_precedes_window() {
        // Previously showing [3,7). Active jumps back to 0.
        assert_eq!(visible_range(3, 0, 7, 4), 0..4);
    }

    #[test]
    fn visible_range_empty_when_no_panes() {
        assert_eq!(visible_range(0, 0, 0, 4), 0..0);
    }

    #[test]
    fn split_pane_areas_single_pane_fills_area() {
        let area = Rect::new(0, 0, 80, 24);
        assert_eq!(split_pane_areas(area, 1), vec![area]);
    }

    #[test]
    fn split_pane_areas_two_panes_side_by_side_when_wide() {
        let area = Rect::new(0, 0, 80, 24);
        let cells = split_pane_areas(area, 2);
        assert_eq!(cells.len(), 2);
        assert_eq!(cells[0].y, cells[1].y);
        assert_ne!(cells[0].x, cells[1].x);
    }

    #[test]
    fn split_pane_areas_two_panes_stacked_when_narrow() {
        let area = Rect::new(0, 0, 30, 24);
        let cells = split_pane_areas(area, 2);
        assert_eq!(cells.len(), 2);
        assert_eq!(cells[0].x, cells[1].x);
        assert_ne!(cells[0].y, cells[1].y);
    }

    #[test]
    fn split_pane_areas_three_panes_two_over_one() {
        let area = Rect::new(0, 0, 80, 24);
        let cells = split_pane_areas(area, 3);
        assert_eq!(cells.len(), 3);
        // First row: two side-by-side cells.
        assert_eq!(cells[0].y, cells[1].y);
        // Second row: one full-width cell below.
        assert!(cells[2].y > cells[0].y);
    }

    #[test]
    fn split_pane_areas_four_panes_is_2x2() {
        let area = Rect::new(0, 0, 80, 24);
        let cells = split_pane_areas(area, 4);
        assert_eq!(cells.len(), 4);
        let rows: std::collections::BTreeSet<u16> = cells.iter().map(|r| r.y).collect();
        assert_eq!(rows.len(), 2);
    }

    #[test]
    fn split_pane_areas_seven_panes_four_then_three() {
        let area = Rect::new(0, 0, 100, 30);
        let cells = split_pane_areas(area, 7);
        assert_eq!(cells.len(), 7);
        let top_row_y = cells[0].y;
        let top_row_count = cells.iter().filter(|r| r.y == top_row_y).count();
        assert_eq!(top_row_count, 4);
    }

    #[test]
    fn split_pane_areas_never_produces_zero_size_cells_when_area_fits() {
        for count in 1..=7 {
            let area = Rect::new(0, 0, 40, 20);
            let cells = split_pane_areas(area, count);
            assert_eq!(cells.len(), count, "count={count}");
            for cell in cells {
                assert!(cell.width > 0 && cell.height > 0, "count={count} cell={cell:?}");
            }
        }
    }

    #[test]
    fn split_pane_areas_empty_for_zero_count() {
        assert!(split_pane_areas(Rect::new(0, 0, 80, 24), 0).is_empty());
    }

    fn buffer_text(buf: &ratatui::buffer::Buffer) -> String {
        buf.content.iter().map(|c| c.symbol()).collect()
    }

    #[test]
    fn single_pane_render_still_has_no_left_border_character() {
        // Regression guard for split-view acceptance criterion 9: with only
        // one pane, render() must take the no-cell-border branch, matching
        // pre-split-view behaviour exactly (clean copy-paste, no `│`).
        let mut app = crate::app::tests::app_with_fake_backend();
        app.terminal.create_pane_with(None, Some("Solo")).unwrap();
        let area = Rect::new(0, 0, 40, 10);
        let mut terminal = Terminal::new(TestBackend::new(40, 10)).unwrap();

        terminal
            .draw(|frame| {
                render(frame, &app, area, Color::Yellow);
            })
            .unwrap();

        let content = content_area(area).unwrap();
        let buf = terminal.backend().buffer();
        for y in content.top()..content.bottom() {
            let cell = buf.cell((content.x, y)).unwrap();
            assert_ne!(
                cell.symbol(),
                "│",
                "single pane must not draw a left border at y={y}"
            );
        }
    }

    #[test]
    fn split_view_renders_multiple_panes_simultaneously() {
        let mut app = crate::app::tests::app_with_fake_backend();
        app.terminal.create_pane_with(None, Some("Alpha")).unwrap();
        app.terminal.create_pane_with(None, Some("Beta")).unwrap();
        let mut terminal = Terminal::new(TestBackend::new(60, 20)).unwrap();

        terminal
            .draw(|frame| {
                render(frame, &app, frame.area(), Color::Yellow);
            })
            .unwrap();

        let text = buffer_text(terminal.backend().buffer());
        assert!(
            text.contains("Alpha") && text.contains("Beta"),
            "expected both pane titles visible at once, got: {text}"
        );
    }

    #[test]
    fn split_view_borders_active_pane_in_accent_color() {
        let mut app = crate::app::tests::app_with_fake_backend();
        app.terminal.create_pane_with(None, Some("Alpha")).unwrap();
        app.terminal.create_pane_with(None, Some("Beta")).unwrap();
        let accent = Color::Yellow;
        let mut terminal = Terminal::new(TestBackend::new(60, 20)).unwrap();

        terminal
            .draw(|frame| {
                render(frame, &app, frame.area(), accent);
            })
            .unwrap();

        let buf = terminal.backend().buffer();
        assert!(
            buf.content.iter().any(|cell| cell.fg == accent),
            "expected the active pane's border/title in accent color"
        );
        assert!(
            buf.content.iter().any(|cell| cell.fg == Color::DarkGray),
            "expected the inactive pane's border in dark gray"
        );
    }

    #[test]
    fn tab_bar_marks_hidden_panes_beyond_max_visible() {
        let mut app = crate::app::tests::app_with_fake_backend();
        for i in 0..5 {
            app.terminal
                .create_pane_with(None, Some(&format!("P{i}")))
                .unwrap();
        }
        assert_eq!(app.terminal.max_visible_normal, 4);
        let mut terminal = Terminal::new(TestBackend::new(80, 20)).unwrap();

        terminal
            .draw(|frame| {
                render(frame, &app, frame.area(), Color::Yellow);
            })
            .unwrap();

        let text = buffer_text(terminal.backend().buffer());
        assert!(
            text.contains('+'),
            "expected a hidden-pane count marker, got: {text}"
        );
    }
}
