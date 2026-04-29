pub mod diff_viewer;
pub mod file_list;
pub mod terminal_tab;

use crate::app::{App, Focus};
use crate::config::LayoutConfig;
use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout},
    style::{Color, Style},
    text::{Line, Span},
    widgets::Paragraph,
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
    // Always reserve 1 row for the status/hint bar.
    let lower_pct = 100u16.saturating_sub(layout.upper_pct).saturating_sub(1);

    let root = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage(layout.upper_pct),
            Constraint::Percentage(lower_pct),
            Constraint::Length(1),
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

    let hint_bar = if let Some(ref msg) = app.status {
        Paragraph::new(Line::from(msg.as_str())).style(Style::default().fg(Color::Red))
    } else {
        let hint = match app.focus {
            Focus::Terminal => {
                " Shift+Tab: upper panel  |  Ctrl+T: new pane  |  F1-F9: switch pane  |  Ctrl+Q: quit "
            }
            _ => " Tab: FileList↔Diff  |  Shift+Tab: terminal  |  j/k: navigate  |  Ctrl+Q: quit ",
        };
        Paragraph::new(Line::from(Span::styled(
            hint,
            Style::default().fg(Color::DarkGray),
        )))
    };
    frame.render_widget(hint_bar, root[2]);
}
