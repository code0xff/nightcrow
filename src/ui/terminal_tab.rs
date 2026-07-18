use crate::app::{App, Focus};
use crate::backend::PaneId;
use crate::runtime::emulator::{CellView, ScreenView};
use crate::runtime::terminal::{MAX_VISIBLE_FULLSCREEN, visible_range};
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

/// Number of panes reachable by a direct `F3`..`F10` jump key. Panes past
/// this index have no jump-key hint in the tab bar (only focus cycling
/// reaches them). Tied to `MAX_VISIBLE_FULLSCREEN` by reference (not just by
/// convention) so the two can never silently drift apart.
const JUMP_KEY_PANE_COUNT: usize = MAX_VISIBLE_FULLSCREEN;

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
/// grid; 5-6 use 3 columns; 7 uses a 4-then-3 row split; 8 is a 2x4 grid.
/// Counts beyond that (not expected given `MAX_VISIBLE_FULLSCREEN`) fall back
/// to a near-square grid. Every returned Rect has at least 1x1 size when
/// `area` is at least `count` cells large, so no cell silently disappears.
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
        8 => vec![4, 4],
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
/// to resize each pane's PTY) reads from it, so a pane's backend/emulator size
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
/// emulator to exactly what `render` draws inside it.
pub(crate) fn visible_pane_content_areas(app: &App, area: Rect) -> Vec<(PaneId, Rect)> {
    let Some((_, content_area)) = terminal_layout(area) else {
        return Vec::new();
    };
    visible_pane_cells(app, content_area)
        .into_iter()
        .map(|cell| (cell.id, cell.content))
        .collect()
}

/// Draw the terminal panel, returning the screen cell the cursor was placed on
/// (`None` when the panel shows no cursor). See `super::draw` for why the
/// position is returned rather than left implicit in the frame.
pub fn render(frame: &mut Frame, app: &App, area: Rect, accent: Color) -> Option<Position> {
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

    let (tab_area, content_area) = terminal_layout(area)?;

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
        return None;
    }

    let mut cursor = None;
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
            cursor = render_cursor(frame, app, cell.id, cell.content);
        }
    }
    cursor
}

/// What one rendered tab-bar segment is, deciding both its style and what a
/// click on it does.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TabSegment {
    /// The `<leader> t: new terminal` legend shown with no panes. Inert.
    Legend,
    /// A pane's tab; a click jumps to this pane index.
    Tab(usize),
    /// A `+N` hidden-pane marker; a click jumps to the nearest hidden pane
    /// on its side (`visible.start - 1` / `visible.end`), which slides the
    /// visible window by exactly one slot via `sync_visible_window` — the
    /// minimal reveal.
    Marker(usize),
}

impl TabSegment {
    /// The pane index a click on this segment jumps to, if any.
    fn click_target(self) -> Option<usize> {
        match self {
            TabSegment::Legend => None,
            TabSegment::Tab(i) | TabSegment::Marker(i) => Some(i),
        }
    }
}

/// The tab bar's rendered segments in draw order. Single source for
/// `render_tab_bar` (which styles them) and `tab_target_at` (which measures
/// them), so the click hit-test cannot drift from the drawn labels.
fn tab_segments(app: &App, visible: std::ops::Range<usize>) -> Vec<(String, TabSegment)> {
    if app.terminal.panes.is_empty() {
        return vec![(
            format!(" {} t: new terminal ", app.leader_label()),
            TabSegment::Legend,
        )];
    }
    // While the terminal fills the body the upper viewer is hidden, so
    // `<prefix> 1..8` address panes 0..7 directly (see
    // `input::prefix_action_fullscreen`); label the tabs with those digits.
    // In the split view the digits `1`/`2` belong to the list/diff, so the
    // pane legend stays on `F3..F10` there.
    let fullscreen = app.terminal.fullscreen.fills_body();
    let hidden_before = visible.start;
    let hidden_after = app.terminal.panes.len().saturating_sub(visible.end);
    let mut segments = Vec::new();
    if hidden_before > 0 {
        segments.push((
            format!(" +{hidden_before} "),
            TabSegment::Marker(visible.start - 1),
        ));
    }
    segments.extend(app.terminal.panes[visible.clone()].iter().enumerate().map(
        |(offset, pane)| {
            let i = visible.start + offset;
            // Panes 0..=7 carry a jump key, so show it as a key legend:
            // `1..8` in fullscreen (both `<prefix> 1..8` and `F1..F8`),
            // `F3..F10` in the split view (`<prefix> 3..9,0`). Panes past the
            // 8th have no jump key, so they carry no hint to avoid implying
            // an unbound shortcut.
            let title = truncate_tab_title(&pane.title, TAB_TITLE_MAX_CHARS);
            let label = if i < JUMP_KEY_PANE_COUNT {
                if fullscreen {
                    format!(" {} {} ", i + 1, title)
                } else {
                    format!(" F{} {} ", i + 3, title)
                }
            } else {
                format!(" {} ", title)
            };
            (label, TabSegment::Tab(i))
        },
    ));
    if hidden_after > 0 {
        segments.push((format!(" +{hidden_after} "), TabSegment::Marker(visible.end)));
    }
    segments
}

/// The pane index a click at screen cell `(x, y)` on the tab bar should
/// jump to: a tab targets its own pane, a `+N` marker the nearest hidden
/// pane on its side. `None` off the tab row, past the last segment, or on
/// the no-panes legend. `area` is the full terminal widget Rect, exactly
/// what `render` receives.
pub(crate) fn tab_target_at(app: &App, area: Rect, x: u16, y: u16) -> Option<usize> {
    let (tab_area, _) = terminal_layout(area)?;
    if !tab_area.contains(Position { x, y }) {
        return None;
    }
    let visible = visible_range(
        app.terminal.visible_start,
        app.terminal.active,
        app.terminal.panes.len(),
        app.terminal.max_visible(),
    );
    let mut cursor = tab_area.x;
    for (text, segment) in tab_segments(app, visible) {
        let width = Span::raw(text.as_str()).width() as u16;
        if x >= cursor && x < cursor + width {
            return segment.click_target();
        }
        cursor += width;
    }
    None
}

fn render_tab_bar(
    frame: &mut Frame,
    app: &App,
    tab_area: Rect,
    accent: Color,
    focused: bool,
    visible: std::ops::Range<usize>,
) {
    let tab_spans: Vec<Span> = tab_segments(app, visible)
        .into_iter()
        .map(|(text, segment)| {
            let style = match segment {
                TabSegment::Tab(i) if i == app.terminal.active && focused => Style::default()
                    .fg(Color::Black)
                    .bg(accent)
                    .add_modifier(Modifier::BOLD),
                TabSegment::Tab(_) => Style::default().fg(Color::Gray),
                TabSegment::Legend | TabSegment::Marker(_) => {
                    Style::default().fg(Color::DarkGray)
                }
            };
            Span::styled(text, style)
        })
        .collect();
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
                let mut style = Style::default();
                let cell = match screen.cell(row, col) {
                    Some(cell) => {
                        // Wide chars (e.g., Hangul) occupy two columns: the
                        // glyph lives on the first cell and a spacer fills
                        // the second. Emitting anything for the spacer would
                        // shift the row by one column.
                        if cell.is_wide_spacer() {
                            continue;
                        }
                        style = cell_to_style(&cell);
                        Some(cell)
                    }
                    None => None,
                };

                if style != run_style {
                    if !run_text.is_empty() {
                        spans.push(Span::styled(std::mem::take(&mut run_text), run_style));
                    }
                    run_style = style;
                }
                match cell {
                    Some(cell) => cell.append_contents(&mut run_text),
                    None => run_text.push(' '),
                }
            }
            if !run_text.is_empty() {
                spans.push(Span::styled(run_text, run_style));
            }
            Line::from(spans)
        })
        .collect()
}

fn render_cursor(frame: &mut Frame, app: &App, pane_id: PaneId, area: Rect) -> Option<Position> {
    if app.focus != Focus::Terminal {
        return None;
    }
    if app.terminal.is_scrolled() {
        return None;
    }

    let screen = app.terminal.screen_for_pane(pane_id)?;
    let position = screen_cursor_position(&screen, area)?;

    frame.set_cursor_position(position);
    Some(position)
}

fn screen_cursor_position(screen: &ScreenView<'_>, area: Rect) -> Option<Position> {
    if area.height == 0 || area.width == 0 {
        return None;
    }

    // Embedded CLIs such as Claude can leave DECTCEM hide-cursor mode enabled
    // while still expecting an outer terminal host to expose the input point.
    // For the focused terminal pane, keep the host cursor visible at the
    // emulator's tracked cursor position instead of honoring the inner app's
    // hide flag.
    let (row, col) = screen.cursor_position();
    Some(Position::new(
        area.x.saturating_add(col.min(area.width.saturating_sub(1))),
        area.y
            .saturating_add(row.min(area.height.saturating_sub(1))),
    ))
}

fn cell_to_style(cell: &CellView<'_>) -> Style {
    let mut style = Style::default().fg(cell.fg()).bg(cell.bg());
    if cell.bold() {
        style = style.add_modifier(Modifier::BOLD);
    }
    if cell.italic() {
        style = style.add_modifier(Modifier::ITALIC);
    }
    if cell.underline() {
        style = style.add_modifier(Modifier::UNDERLINED);
    }
    if cell.dim() {
        style = style.add_modifier(Modifier::DIM);
    }
    // Reverse video is how vim visual mode, fzf's cursor, and less's search
    // hit mark selections. Without it those selections render as plain text.
    if cell.inverse() {
        style = style.add_modifier(Modifier::REVERSED);
    }
    style
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::tests::app_with_files;
    use ratatui::{Terminal, backend::TestBackend};

    #[test]
    fn maps_screen_cursor_to_render_area() {
        let mut emulator = crate::runtime::emulator::PaneEmulator::new(3, 10, 0);
        emulator.process(b"\x1b[2;4H");

        let position =
            screen_cursor_position(&emulator.view(), Rect::new(20, 10, 10, 3)).unwrap();

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
        let mut emulator = crate::runtime::emulator::PaneEmulator::new(3, 10, 0);
        emulator.process(b"\x1b[?25l\x1b[2;4H");

        let position =
            screen_cursor_position(&emulator.view(), Rect::new(20, 10, 10, 3)).unwrap();

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
    fn split_pane_areas_eight_panes_is_2x4() {
        let area = Rect::new(0, 0, 100, 30);
        let cells = split_pane_areas(area, 8);
        assert_eq!(cells.len(), 8);
        let rows: std::collections::BTreeSet<u16> = cells.iter().map(|r| r.y).collect();
        assert_eq!(rows.len(), 2);
        let top_row_y = cells[0].y;
        let top_row_count = cells.iter().filter(|r| r.y == top_row_y).count();
        assert_eq!(top_row_count, 4);
    }

    #[test]
    fn split_pane_areas_never_produces_zero_size_cells_when_area_fits() {
        for count in 1..=8 {
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
            !buf.content
                .iter()
                .any(|cell| cell.fg == accent || cell.fg == Color::White),
            "terminal must not show accent or white anywhere while unfocused"
        );
        assert!(
            !buf.content
                .iter()
                .any(|cell| cell.modifier.contains(Modifier::BOLD) && cell.bg == accent),
            "active pane tab must not carry an accent-bolded highlight while unfocused"
        );
    }

    /// x column where the `nth` tab-bar segment starts, measured with the
    /// same builder and widths the renderer and hit-test use.
    fn tab_segment_x(app: &App, area: Rect, nth: usize) -> u16 {
        let (tab_area, _) = terminal_layout(area).unwrap();
        let visible = visible_range(
            app.terminal.visible_start,
            app.terminal.active,
            app.terminal.panes.len(),
            app.terminal.max_visible(),
        );
        let segments = tab_segments(app, visible);
        assert!(nth < segments.len(), "segment {nth} must exist");
        tab_area.x
            + segments
                .iter()
                .take(nth)
                .map(|(text, _)| Span::raw(text.as_str()).width() as u16)
                .sum::<u16>()
    }

    #[test]
    fn tab_target_at_resolves_tabs_and_hidden_markers() {
        let mut app = crate::app::tests::app_with_fake_backend();
        app.terminal.max_visible_normal = 2;
        for i in 0..4 {
            app.terminal
                .create_pane_with(None, Some(&format!("P{i}")))
                .unwrap();
        }
        // Creation leaves pane 3 active with a 2-pane window: [2, 4).
        let area = Rect::new(0, 0, 80, 20);
        let (tab_area, _) = terminal_layout(area).unwrap();
        let y = tab_area.y;

        // Segment 0 is the ` +2 ` marker → nearest hidden pane on the left.
        assert_eq!(tab_target_at(&app, area, tab_segment_x(&app, area, 0), y), Some(1));
        // Segments 1 and 2 are the visible tabs for panes 2 and 3.
        assert_eq!(tab_target_at(&app, area, tab_segment_x(&app, area, 1), y), Some(2));
        assert_eq!(tab_target_at(&app, area, tab_segment_x(&app, area, 2), y), Some(3));
        // Past the last segment and off the tab row: no target.
        assert_eq!(tab_target_at(&app, area, tab_area.right() - 1, y), None);
        assert_eq!(tab_target_at(&app, area, tab_segment_x(&app, area, 1), y + 1), None);
    }

    #[test]
    fn tab_target_at_right_marker_reveals_the_next_hidden_pane() {
        let mut app = crate::app::tests::app_with_fake_backend();
        app.terminal.max_visible_normal = 2;
        for i in 0..4 {
            app.terminal
                .create_pane_with(None, Some(&format!("P{i}")))
                .unwrap();
        }
        // Jump back to pane 0: window slides to [0, 2), marker sits on the right.
        app.terminal.active = 0;
        app.terminal.sync_visible_window();
        let area = Rect::new(0, 0, 80, 20);
        let (tab_area, _) = terminal_layout(area).unwrap();

        // Segments: tab 0, tab 1, ` +2 ` marker → nearest hidden pane index 2.
        let x = tab_segment_x(&app, area, 2);
        assert_eq!(tab_target_at(&app, area, x, tab_area.y), Some(2));
    }

    #[test]
    fn tab_target_agrees_with_the_rendered_buffer_not_just_the_builder() {
        // Independent cross-check: find the second tab's jump-key label in
        // the *rendered* buffer and hit-test at that column. Catches any
        // renderer vs hit-test segmentation drift the builder-based
        // position helper cannot see.
        let mut app = crate::app::tests::app_with_fake_backend();
        app.terminal.create_pane_with(None, Some("Alpha")).unwrap();
        app.terminal.create_pane_with(None, Some("Beta")).unwrap();
        let area = Rect::new(0, 0, 80, 20);
        let mut terminal = Terminal::new(TestBackend::new(80, 20)).unwrap();
        terminal
            .draw(|frame| {
                render(frame, &app, area, Color::Yellow);
            })
            .unwrap();

        let (tab_area, _) = terminal_layout(area).unwrap();
        let buf = terminal.backend().buffer();
        let cells: Vec<&str> = (0..buf.area.width)
            .map(|x| buf[(x, tab_area.y)].symbol())
            .collect();
        let x = (0..cells.len())
            .find(|&i| cells[i..].concat().starts_with("F4 Beta"))
            .expect("second tab rendered") as u16;

        assert_eq!(tab_target_at(&app, area, x, tab_area.y), Some(1));
    }

    #[test]
    fn tab_target_at_none_on_the_no_pane_legend() {
        let app = crate::app::tests::app_with_fake_backend();
        let area = Rect::new(0, 0, 80, 20);
        let (tab_area, _) = terminal_layout(area).unwrap();

        assert_eq!(tab_target_at(&app, area, tab_area.x + 2, tab_area.y), None);
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

    #[test]
    fn tab_bar_labels_panes_with_f_keys_in_split_view() {
        let mut app = crate::app::tests::app_with_fake_backend();
        app.terminal.create_pane_with(None, Some("Alpha")).unwrap();
        let mut terminal = Terminal::new(TestBackend::new(60, 20)).unwrap();

        terminal
            .draw(|frame| {
                render(frame, &app, frame.area(), Color::Yellow);
            })
            .unwrap();

        let text = buffer_text(terminal.backend().buffer());
        assert!(
            text.contains("F3 Alpha"),
            "split view must label the first pane with its F3 jump key, got: {text}"
        );
    }

    #[test]
    fn tab_bar_labels_panes_with_digits_in_fullscreen() {
        // Fullscreen hides the viewer, so the pane legend switches to the
        // `<prefix> 1..8` digits that address panes there.
        let mut app = crate::app::tests::app_with_fake_backend();
        app.terminal.create_pane_with(None, Some("Alpha")).unwrap();
        app.terminal.create_pane_with(None, Some("Beta")).unwrap();
        app.terminal.fullscreen = crate::runtime::terminal::TerminalFullscreen::Grid;
        let mut terminal = Terminal::new(TestBackend::new(60, 20)).unwrap();

        terminal
            .draw(|frame| {
                render(frame, &app, frame.area(), Color::Yellow);
            })
            .unwrap();

        let text = buffer_text(terminal.backend().buffer());
        assert!(
            text.contains("1 Alpha") && text.contains("2 Beta"),
            "fullscreen must label panes with their <prefix> digits, got: {text}"
        );
        assert!(
            !text.contains("F3"),
            "fullscreen must not show the split-view F-key legend, got: {text}"
        );
    }
}
