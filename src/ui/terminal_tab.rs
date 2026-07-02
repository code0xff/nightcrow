use crate::app::{App, Focus};
use crate::backend::PaneId;
use crate::runtime::terminal::visible_range;
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

/// One visible split-view cell: `outer` is the full grid cell (border +
/// content), `content` is where the PTY screen actually draws. For the
/// single-pane case `outer == content` and `bordered` is `false` — no cell
/// border is drawn, matching pre-split-view rendering exactly.
struct VisiblePaneCell {
    id: PaneId,
    outer: Rect,
    content: Rect,
    bordered: bool,
}

/// Lay out every currently visible pane inside `content_area` (the terminal
/// body, i.e. below the tab row). This is the single source of truth for
/// pane sizing: `render` draws from it and `visible_pane_content_areas` (used
/// to resize each pane's PTY) reads from it, so a pane's backend/vt100 size
/// always matches what's actually drawn on screen.
fn visible_pane_cells(app: &App, content_area: Rect) -> Vec<VisiblePaneCell> {
    let pane_count = app.terminal.panes.len();
    let visible = visible_range(
        app.terminal.visible_start,
        app.terminal.active,
        pane_count,
        app.terminal.max_visible(),
    );
    if visible.is_empty() {
        return Vec::new();
    }
    let visible_ids: Vec<PaneId> = app.terminal.panes[visible].iter().map(|p| p.id).collect();

    if visible_ids.len() == 1 {
        return vec![VisiblePaneCell {
            id: visible_ids[0],
            outer: content_area,
            content: content_area,
            bordered: false,
        }];
    }

    let outers = split_pane_areas(content_area, visible_ids.len());
    visible_ids
        .into_iter()
        .zip(outers)
        .map(|(id, outer)| VisiblePaneCell {
            id,
            outer,
            content: Block::default().borders(Borders::ALL).inner(outer),
            bordered: true,
        })
        .collect()
}

/// Content Rect (post border) for every currently visible pane, keyed by
/// pane id. Used by the main loop to resize each pane's backend PTY and
/// vt100 parser to exactly what `render` draws inside it.
pub(crate) fn visible_pane_content_areas(app: &App, area: Rect) -> Vec<(PaneId, Rect)> {
    let Some((_, content_area)) = terminal_layout(area) else {
        return Vec::new();
    };
    visible_pane_cells(app, content_area)
        .into_iter()
        .map(|cell| (cell.id, cell.content))
        .collect()
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
    render_tab_bar(frame, app, tab_area, accent, focused, visible.clone());

    let cells = visible_pane_cells(app, content_area);
    if cells.is_empty() {
        let screen_lines = vec![Line::from(Span::styled(
            format!(" No terminal — press {} t to open one ", app.leader_label()),
            Style::default().fg(Color::DarkGray),
        ))];
        frame.render_widget(Paragraph::new(screen_lines), content_area);
        return;
    }

    for (offset, cell) in cells.iter().enumerate() {
        let i = visible.start + offset;
        let is_active = i == app.terminal.active;
        if cell.bordered {
            // `accent` means "this is where your keystrokes go right now" —
            // reserved for Focus::Terminal, matching FileList/DiffViewer.
            // Without real focus, the active pane must look identical to an
            // inactive one (plain DarkGray) — any brighter treatment reads
            // as focused when it isn't.
            let pane_border_style = if is_active && focused {
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
            frame.render_widget(cell_block, cell.outer);
        }
        if cell.content.width == 0 || cell.content.height == 0 {
            continue;
        }
        let screen_lines =
            build_screen_lines(app, cell.id, cell.content.height, cell.content.width);
        frame.render_widget(Paragraph::new(screen_lines), cell.content);
        if is_active {
            render_cursor(frame, app, cell.id, cell.content);
        }
    }
}

fn render_tab_bar(
    frame: &mut Frame,
    app: &App,
    tab_area: Rect,
    accent: Color,
    focused: bool,
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
                let style = if i == app.terminal.active && focused {
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
        let (_, content) = terminal_layout(area).unwrap();

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
                assert!(
                    cell.width > 0 && cell.height > 0,
                    "count={count} cell={cell:?}"
                );
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

        let (_, content) = terminal_layout(area).unwrap();
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
        app.focus = Focus::Terminal;
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
    fn split_view_active_pane_matches_inactive_style_when_terminal_unfocused() {
        // Regression guard: accent (and any brighter stand-in for it) must
        // mean "keystrokes go here right now". When Diff/FileList holds
        // focus, the terminal's active pane must render pixel-identical to
        // an inactive pane — no accent, no bold, no lighter gray — otherwise
        // it still reads as focused when it isn't.
        let mut app = crate::app::tests::app_with_fake_backend();
        app.terminal.create_pane_with(None, Some("Alpha")).unwrap();
        app.terminal.create_pane_with(None, Some("Beta")).unwrap();
        app.focus = Focus::DiffViewer;
        let accent = Color::Yellow;
        let mut terminal = Terminal::new(TestBackend::new(60, 20)).unwrap();

        terminal
            .draw(|frame| {
                render(frame, &app, frame.area(), accent);
            })
            .unwrap();

        let buf = terminal.backend().buffer();
        assert!(
            !buf.content.iter().any(|cell| cell.fg == accent || cell.fg == Color::White),
            "terminal must not show accent or white anywhere while unfocused"
        );
        assert!(
            !buf.content.iter().any(|cell| cell.modifier.contains(Modifier::BOLD)
                && cell.bg == accent),
            "active pane tab must not carry an accent-bolded highlight while unfocused"
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
