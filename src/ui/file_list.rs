use crate::app::{App, Focus};
use crate::git::diff::ChangeStatus;
use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem, ListState, Paragraph},
};

pub fn render(frame: &mut Frame, app: &App, area: Rect) {
    let focused = app.focus == Focus::FileList;
    let border_style = super::focused_border_style(focused);

    let show_search = app.search_active || !app.search_query.is_empty();

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
            let f = &app.files[idx];
            let symbol = f.status.symbol();
            let color = match f.status {
                ChangeStatus::Added => Color::Green,
                ChangeStatus::Deleted => Color::Red,
                ChangeStatus::Renamed => Color::Cyan,
                ChangeStatus::Untracked => Color::DarkGray,
                ChangeStatus::Modified => Color::Yellow,
            };
            let line = Line::from(vec![
                Span::styled(format!("{symbol} "), Style::default().fg(color)),
                Span::raw(&f.path),
            ]);
            ListItem::new(line)
        })
        .collect();

    let title = if show_search {
        format!(" Files ({}/{}) ", match_count, app.files.len())
    } else if app.files.is_empty() {
        " Files (no changes) ".to_string()
    } else {
        " Files ".to_string()
    };

    let list = List::new(items)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title(title)
                .border_style(border_style),
        )
        .highlight_style(
            Style::default()
                .bg(Color::DarkGray)
                .add_modifier(Modifier::BOLD),
        )
        .highlight_symbol("> ");

    let mut state = ListState::default();
    if !filtered_indices.is_empty()
        && let Some(pos) = filtered_indices.iter().position(|&i| i == app.selected)
    {
        state.select(Some(pos));
    }

    frame.render_stateful_widget(list, list_area, &mut state);

    if let Some(sa) = search_area {
        let cursor = if app.search_active { "█" } else { "" };
        let search_style = if app.search_active {
            Style::default().fg(Color::Yellow)
        } else {
            Style::default().fg(Color::DarkGray)
        };
        frame.render_widget(
            Paragraph::new(format!("/{}{}", app.search_query, cursor)).style(search_style),
            sa,
        );
    }
}
