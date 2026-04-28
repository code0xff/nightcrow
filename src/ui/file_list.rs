use crate::app::{App, Focus};
use crate::git::diff::ChangeStatus;
use ratatui::{
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem, ListState},
    Frame,
};

pub fn render(frame: &mut Frame, app: &App, area: Rect) {
    let focused = app.focus == Focus::FileList;
    let border_style = if focused {
        Style::default().fg(Color::Yellow)
    } else {
        Style::default().fg(Color::DarkGray)
    };

    let items: Vec<ListItem> = app
        .files
        .iter()
        .map(|f| {
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

    let title = if app.files.is_empty() {
        " Files (no changes) "
    } else {
        " Files "
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
    if !app.files.is_empty() {
        state.select(Some(app.selected));
    }

    frame.render_stateful_widget(list, area, &mut state);
}
