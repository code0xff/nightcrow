use crate::app::{App, Focus};
use crate::backend::PaneId;
use crate::runtime::emulator::{CellView, ScreenView};
use ratatui::{
    Frame,
    buffer::{Buffer, CellWidth},
    layout::{Position, Rect},
    style::{Color, Modifier, Style},
    widgets::Widget,
};

/// Draws one pane's emulated grid into the frame buffer, one cell at a time.
///
/// The `Line`/`Paragraph` route needs a `String` per style run and then has
/// ratatui re-segment every row into graphemes, though the emulator already
/// knows the grapheme boundaries: each screen cell is one grapheme, with its
/// combining characters attached.
pub(crate) struct ScreenCells<'a> {
    app: &'a App,
    pane: PaneId,
}

impl<'a> ScreenCells<'a> {
    pub(crate) fn new(app: &'a App, pane: PaneId) -> Self {
        Self { app, pane }
    }
}

impl Widget for ScreenCells<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let Some(screen) = self.app.terminal.screen_for_pane(self.pane) else {
            write_placeholder(area, buf);
            return;
        };
        let (screen_rows, screen_cols) = screen.size();
        let rows = area.height.min(screen_rows);
        let cols = area.width.min(screen_cols);

        let mut symbol = String::new();
        for row in 0..rows {
            let y = area.y.saturating_add(row);
            for col in 0..cols {
                let Some(cell) = screen.cell(row, col) else {
                    break;
                };
                // Wide characters occupy two columns: the glyph lives on the
                // first cell and a spacer fills the second; writing anything
                // for the spacer would shift the rest of the row.
                if cell.is_wide_spacer() {
                    continue;
                }
                symbol.clear();
                cell.append_contents(&mut symbol);
                let width = symbol.cell_width();
                // Nothing a terminal can show here, and no column of its own.
                if width == 0 {
                    continue;
                }
                let x = area.x.saturating_add(col);
                // A glyph that would straddle the edge is dropped whole, as a
                // clipped line drops it, rather than half-drawn.
                if x.saturating_add(width) > area.right() {
                    break;
                }
                let Some(target) = buf.cell_mut((x, y)) else {
                    break;
                };
                target.set_symbol(&symbol).set_style(cell_to_style(&cell));
                if width > 1
                    && let Some(covered) = buf.cell_mut((x.saturating_add(1), y))
                {
                    covered.reset();
                }
            }
        }
    }
}

fn write_placeholder(area: Rect, buf: &mut Buffer) {
    let text = " (no output) ";
    let style = Style::default().fg(Color::DarkGray);
    let mut x = area.x;
    for ch in text.chars() {
        if x >= area.right() {
            break;
        }
        let mut encoded = [0u8; 4];
        if let Some(cell) = buf.cell_mut((x, area.y)) {
            cell.set_symbol(ch.encode_utf8(&mut encoded))
                .set_style(style);
        }
        x = x.saturating_add(1);
    }
}

/// The reference renderer for the same rows, kept to prove the widget draws
/// them identically. It goes through `Paragraph`, which is what the frame
/// loop used to do for every visible pane.
#[cfg(test)]
pub(crate) fn build_screen_lines(
    app: &App,
    pane_id: PaneId,
    rows: u16,
    cols: u16,
) -> Vec<ratatui::text::Line<'static>> {
    use ratatui::text::{Line, Span};

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
                        // Wide chars occupy two columns: the glyph lives on the
                        // first cell and a spacer fills the second; emitting
                        // anything for the spacer would shift the row.
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

pub(crate) fn render_cursor(frame: &mut Frame, app: &App, pane_id: PaneId, area: Rect) {
    if app.focus != Focus::Terminal || app.terminal.is_scrolled() {
        return;
    }
    let Some(screen) = app.terminal.screen_for_pane(pane_id) else {
        return;
    };
    let Some(position) = screen_cursor_position(&screen, area) else {
        return;
    };
    frame.set_cursor_position(position);
}

pub(crate) fn screen_cursor_position(screen: &ScreenView<'_>, area: Rect) -> Option<Position> {
    if area.height == 0 || area.width == 0 {
        return None;
    }

    // Embedded CLIs such as Claude can leave DECTCEM hide-cursor mode enabled
    // while still expecting an outer terminal host to expose the input point,
    // so keep the host cursor visible at the emulator's tracked position.
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
