use crate::app::{App, Focus};
use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Style},
    text::{Line, Span},
    widgets::ListItem,
};

pub fn render(frame: &mut Frame, app: &App, area: Rect, accent: Color) {
    let focused = app.focus == Focus::FileList;
    let border_style = super::focused_border_style(focused, accent);

    let show_search = app.status_view.search_active || !app.status_view.search_query.is_empty();

    let (list_area, search_area) = if show_search {
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Min(0), Constraint::Length(1)])
            .split(area);
        (chunks[0], Some(chunks[1]))
    } else {
        (area, None)
    };

    let filtered_indices = app.filtered_indices();
    let match_count = filtered_indices.len();

    let items: Vec<ListItem> = filtered_indices
        .iter()
        .map(|&idx| {
            let f = &app.status_view.files[idx];
            let symbol = f.status.symbol();
            let color = super::status_color(f.status);
            let path: &str = if app.status_view.file_scroll_x == 0 {
                &f.path
            } else {
                let byte_off = f
                    .path
                    .char_indices()
                    .nth(app.status_view.file_scroll_x)
                    .map(|(b, _)| b)
                    .unwrap_or(f.path.len());
                &f.path[byte_off..]
            };
            let line = Line::from(vec![
                Span::styled(format!("{symbol} "), Style::default().fg(color)),
                Span::raw(path),
            ]);
            ListItem::new(line)
        })
        .collect();

    let title = if show_search {
        format!(" Files ({}/{}) ", match_count, app.status_view.files.len())
    } else if app.status_view.files.is_empty() {
        " Files (no changes) ".to_string()
    } else {
        " Files ".to_string()
    };

    let selected_pos = filtered_indices
        .iter()
        .position(|&i| i == app.status_view.selected);
    super::render_selectable_list(frame, list_area, title, items, selected_pos, border_style);

    if let Some(sa) = search_area {
        super::render_search_bar(
            frame,
            &app.status_view.search_query,
            app.status_view.search_active,
            sa,
            accent,
        );
    }
}
