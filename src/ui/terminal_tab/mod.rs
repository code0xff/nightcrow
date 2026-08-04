mod cells;
mod layout;
mod recovery;
mod screen;
mod tab_bar;
#[cfg(test)]
mod tests;

pub(crate) use cells::visible_pane_content_areas;
pub(crate) use tab_bar::tab_target_at;

use crate::app::{App, Focus, leader_label_of};
use crate::runtime::terminal::visible_range;
use crate::ui::terminal_tab::cells::visible_pane_cells;
use crate::ui::terminal_tab::layout::{TERMINAL_BORDERS, terminal_layout};
use crate::ui::terminal_tab::recovery::pane_label;
use crate::ui::terminal_tab::screen::{build_screen_lines, render_cursor};
use crate::ui::terminal_tab::tab_bar::render_tab_bar;
use ratatui::{
    Frame,
    layout::Rect,
    style::{Color, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph},
};

/// Draw the terminal panel, placing the host cursor on the active pane when
/// that pane shows one.
pub fn render(frame: &mut Frame, app: &App, area: Rect, accent: Color) {
    let focused = app.focus == Focus::Terminal;
    let border_style = super::focused_border_style(focused, accent);

    let label = if app.terminal.is_scrolled() {
        " Terminal [SCROLL — shift+pgdn: down | input: live] "
    } else {
        " Terminal "
    };
    // The upper panes draw a `┌` corner that pushes their title text in by one
    // column (`┌ ^F 1 Files`). This pane has no left border, so a border-styled
    // `─` stands in for that corner — it keeps `Terminal` column-aligned with
    // `^F 1 Files` / `^F 2 Diff` above and makes the line start flush at the edge.
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
        let hint = Line::from(Span::styled(
            format!(
                "No terminal — press {} t to open one",
                leader_label_of(app.interaction.leader)
            ),
            Style::default().fg(Color::DarkGray),
        ));
        crate::ui::splash::draw_idle(frame, content_area, accent, hint);
        return;
    }

    for (offset, cell) in cells.iter().enumerate() {
        let i = visible.start + offset;
        let is_active = i == app.terminal.active;
        if cell.bordered {
            // `accent` means "this is where your keystrokes go right now" —
            // reserved for Focus::Terminal. Without real focus, the active
            // pane must look identical to an inactive one (plain DarkGray) —
            // any brighter treatment reads as focused when it isn't.
            let pane_border_style = if is_active && focused {
                Style::default().fg(accent)
            } else {
                Style::default().fg(Color::DarkGray)
            };
            let pane_title = pane_label(app, i);
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
