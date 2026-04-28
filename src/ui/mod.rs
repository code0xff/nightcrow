pub mod diff_viewer;
pub mod file_list;
pub mod terminal_tab;

use crate::app::App;
use ratatui::{
    layout::{Constraint, Direction, Layout},
    Frame,
};
use syntect::highlighting::ThemeSet;
use syntect::parsing::SyntaxSet;

pub fn draw(frame: &mut Frame, app: &App, ss: &SyntaxSet, ts: &ThemeSet) {
    let status_height: u16 = if app.status.is_some() { 1 } else { 0 };

    let root = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage(55),
            Constraint::Percentage(45 - status_height),
            Constraint::Length(status_height),
        ])
        .split(frame.area());

    let upper = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(25), Constraint::Percentage(75)])
        .split(root[0]);

    file_list::render(frame, app, upper[0]);
    diff_viewer::render(frame, app, upper[1], ss, ts);
    terminal_tab::render(frame, root[1]);

    if let Some(ref msg) = app.status {
        use ratatui::{
            style::{Color, Style},
            text::Line,
            widgets::Paragraph,
        };
        let status_bar = Paragraph::new(Line::from(msg.as_str()))
            .style(Style::default().fg(Color::Red));
        frame.render_widget(status_bar, root[2]);
    }
}
