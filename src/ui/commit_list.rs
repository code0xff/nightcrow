use crate::app::{App, Focus};
use ratatui::{
    Frame,
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem, ListState},
};
use std::time::{SystemTime, UNIX_EPOCH};

fn format_relative_time(ts: i64) -> String {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);
    let secs = (now - ts).max(0);
    if secs < 60 {
        format!("{secs}s")
    } else if secs < 3600 {
        format!("{}m", secs / 60)
    } else if secs < 86400 {
        format!("{}h", secs / 3600)
    } else if secs < 86400 * 30 {
        format!("{}d", secs / 86400)
    } else if secs < 86400 * 365 {
        format!("{}mo", secs / (86400 * 30))
    } else {
        format!("{}y", secs / (86400 * 365))
    }
}

pub fn render(frame: &mut Frame, app: &App, area: Rect) {
    let focused = app.focus == Focus::FileList;
    let border_style = if focused {
        Style::default().fg(Color::Yellow)
    } else {
        Style::default().fg(Color::DarkGray)
    };

    let items: Vec<ListItem> = app
        .commits
        .iter()
        .map(|entry| {
            let time_str = format_relative_time(entry.time);
            let author_short: String = entry.author.chars().take(10).collect();
            let line = Line::from(vec![
                Span::styled(
                    format!("{} ", entry.short_id),
                    Style::default().fg(Color::Yellow),
                ),
                Span::styled(
                    format!("{:>4} ", time_str),
                    Style::default().fg(Color::DarkGray),
                ),
                Span::styled(
                    format!("{:<10} ", author_short),
                    Style::default().fg(Color::Cyan),
                ),
                Span::raw(entry.summary.as_str()),
            ]);
            ListItem::new(line)
        })
        .collect();

    let title = if app.commits.is_empty() {
        " Log (no commits) ".to_string()
    } else {
        format!(" Log ({}) ", app.commits.len())
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
    if !app.commits.is_empty() {
        state.select(Some(app.log_selected));
    }

    frame.render_stateful_widget(list, area, &mut state);
}
