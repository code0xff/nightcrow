use crate::app::{App, Focus};
use crate::backend::PaneId;
use crate::runtime::emulator::{CellView, ScreenView};
use ratatui::{
    Frame,
    layout::{Position, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
};

pub(crate) fn build_screen_lines(
    app: &App,
    pane_id: PaneId,
    rows: u16,
    cols: u16,
) -> Vec<Line<'static>> {
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
                        // Wide chars (e.g., Hangul) occupy two columns: the glyph
                        // lives on the first cell and a spacer fills the second.
                        // Emitting anything for the spacer would shift the row by one
                        // column.
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

pub(crate) fn render_cursor(
    frame: &mut Frame,
    app: &App,
    pane_id: PaneId,
    area: Rect,
) -> Option<Position> {
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

pub(crate) fn screen_cursor_position(screen: &ScreenView<'_>, area: Rect) -> Option<Position> {
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
