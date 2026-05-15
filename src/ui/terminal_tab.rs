use crate::app::{App, Focus};
use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Position, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph},
};

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

pub fn render(frame: &mut Frame, app: &mut App, area: Rect, accent: Color) {
    let focused = app.focus == Focus::Terminal;
    let border_style = super::focused_border_style(focused, accent);

    let title = if app.terminal.is_scrolled() {
        " Terminal [SCROLL — shift+pgdn: down | input: live] "
    } else {
        " Terminal "
    };
    let block = Block::default()
        .borders(Borders::ALL)
        .title(title)
        .border_style(border_style);

    let inner = block.inner(area);
    frame.render_widget(block, area);

    if inner.height == 0 || inner.width == 0 {
        return;
    }

    // Tab bar (1 row) + terminal content
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(1), Constraint::Min(0)])
        .split(inner);

    // Update stored terminal size so App can resize panes if needed
    let content_area = chunks[1];
    app.terminal
        .resize_panes(content_area.height, content_area.width);

    // ── Tab bar ──────────────────────────────────────────────
    let tab_spans: Vec<Span> = if app.terminal.panes.is_empty() {
        vec![Span::styled(
            " ctrl+t: new terminal ",
            Style::default().fg(Color::DarkGray),
        )]
    } else {
        app.terminal
            .panes
            .iter()
            .enumerate()
            .map(|(i, pane)| {
                let style = if i == app.terminal.active {
                    Style::default()
                        .fg(Color::Black)
                        .bg(accent)
                        .add_modifier(Modifier::BOLD)
                } else {
                    Style::default().fg(Color::DarkGray)
                };
                // F3..=F9 are wired to panes 0..=6 in `input::map_key`; show
                // the binding so the tab bar doubles as a key legend.
                let key_hint = if i < 7 {
                    format!("F{}", i + 3)
                } else {
                    format!("{}", i + 1)
                };
                let title = truncate_tab_title(&pane.title, TAB_TITLE_MAX_CHARS);
                Span::styled(format!(" {} {} ", key_hint, title), style)
            })
            .collect()
    };
    frame.render_widget(Paragraph::new(Line::from(tab_spans)), chunks[0]);

    // ── Terminal screen ───────────────────────────────────────
    app.terminal.sync_scroll();
    let screen_lines = build_screen_lines(app, content_area.height, content_area.width);
    frame.render_widget(Paragraph::new(screen_lines), content_area);
    render_cursor(frame, app, content_area);
}

fn build_screen_lines(app: &App, rows: u16, cols: u16) -> Vec<Line<'static>> {
    let Some(screen) = app.active_screen() else {
        return vec![Line::from(Span::styled(
            " No terminal — press ctrl+t to open one ",
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

fn render_cursor(frame: &mut Frame, app: &App, area: Rect) {
    if app.focus != Focus::Terminal {
        return;
    }
    if app.terminal.is_scrolled() {
        return;
    }

    let Some(screen) = app.active_screen() else {
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
}
