pub mod diff_viewer;
pub mod file_list;
pub mod terminal_tab;

use crate::app::App;
use crate::config::LayoutConfig;
use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout},
};
use syntect::highlighting::ThemeSet;
use syntect::parsing::SyntaxSet;

pub fn draw(
    frame: &mut Frame,
    app: &mut App,
    ss: &SyntaxSet,
    ts: &ThemeSet,
    layout: &LayoutConfig,
) {
    let status_height: u16 = if app.status.is_some() { 1 } else { 0 };
    let lower_pct = 100u16
        .saturating_sub(layout.upper_pct)
        .saturating_sub(status_height);

    let root = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage(layout.upper_pct),
            Constraint::Percentage(lower_pct),
            Constraint::Length(status_height),
        ])
        .split(frame.area());

    let file_list_pct = layout.file_list_pct;
    let diff_pct = 100u16.saturating_sub(file_list_pct);
    let upper = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage(file_list_pct),
            Constraint::Percentage(diff_pct),
        ])
        .split(root[0]);

    file_list::render(frame, app, upper[0]);
    diff_viewer::render(frame, app, upper[1], ss, ts);
    terminal_tab::render(frame, app, root[1]);

    if let Some(ref msg) = app.status {
        use ratatui::{
            style::{Color, Style},
            text::Line,
            widgets::Paragraph,
        };
        let status_bar =
            Paragraph::new(Line::from(msg.as_str())).style(Style::default().fg(Color::Red));
        frame.render_widget(status_bar, root[2]);
    }
}
