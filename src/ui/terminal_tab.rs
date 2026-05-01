use crate::app::{App, Focus};
use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Position, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph},
};

pub fn render(frame: &mut Frame, app: &mut App, area: Rect) {
    let focused = app.focus == Focus::Terminal;
    let border_style = if focused {
        Style::default().fg(Color::Yellow)
    } else {
        Style::default().fg(Color::DarkGray)
    };

    let title = if app.is_terminal_scrolled() {
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
    app.resize_terminal_panes(content_area.height, content_area.width);

    // ── Tab bar ──────────────────────────────────────────────
    let tab_spans: Vec<Span> = if app.terminal_panes.is_empty() {
        vec![Span::styled(
            " ctrl+t: new terminal ",
            Style::default().fg(Color::DarkGray),
        )]
    } else {
        app.terminal_panes
            .iter()
            .enumerate()
            .map(|(i, pane)| {
                let style = if i == app.active_pane {
                    Style::default()
                        .fg(Color::Black)
                        .bg(Color::Yellow)
                        .add_modifier(Modifier::BOLD)
                } else {
                    Style::default().fg(Color::DarkGray)
                };
                Span::styled(format!(" {} {} ", i + 1, pane.title), style)
            })
            .collect()
    };
    frame.render_widget(Paragraph::new(Line::from(tab_spans)), chunks[0]);

    // ── Terminal screen ───────────────────────────────────────
    app.sync_terminal_scroll();
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
                let (text, style) = match screen.cell(row, col) {
                    Some(cell) => {
                        // Wide chars (e.g., Hangul) occupy two columns: vt100
                        // stores the glyph on the first cell and an empty
                        // continuation on the second. Emitting anything for
                        // the continuation would shift the row by one column.
                        if cell.is_wide_continuation() {
                            continue;
                        }
                        let t = if cell.contents().is_empty() {
                            " ".to_string()
                        } else {
                            cell.contents().to_string()
                        };
                        (t, cell_to_style(cell))
                    }
                    None => (" ".to_string(), Style::default()),
                };

                if style == run_style {
                    run_text.push_str(&text);
                } else {
                    if !run_text.is_empty() {
                        spans.push(Span::styled(std::mem::take(&mut run_text), run_style));
                    }
                    run_style = style;
                    run_text = text;
                }
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
        area.x + col.min(area.width - 1),
        area.y + row.min(area.height - 1),
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
    fn keeps_cursor_visible_when_terminal_requests_hide() {
        let mut parser = vt100::Parser::new(3, 10, 0);
        parser.process(b"\x1b[?25l\x1b[2;4H");

        let position = screen_cursor_position(parser.screen(), Rect::new(20, 10, 10, 3)).unwrap();

        assert_eq!(position, Position::new(23, 11));
    }
}
