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
    if app.terminal_fullscreen {
        let root = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Min(0), Constraint::Length(1)])
            .split(frame.area());

        terminal_tab::render(frame, app, root[0]);
        frame.render_widget(render_hint_bar(app), root[1]);
        return;
    }

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
    frame.render_widget(render_hint_bar(app), root[2]);
}

fn render_hint_bar(app: &App) -> Paragraph<'_> {
    if app.repo_input_active {
        return Paragraph::new(Line::from(vec![
            Span::styled("repo: ", Style::default().fg(Color::Yellow)),
            Span::raw(app.repo_input_buf.as_str()),
            Span::styled("█", Style::default().fg(Color::Yellow)),
        ]));
    }
    if let Some(ref msg) = app.status {
        return Paragraph::new(Line::from(msg.as_str())).style(Style::default().fg(Color::Red));
    }
    if app.terminal_fullscreen {
        return Paragraph::new(Line::from(Span::styled(
            " ctrl+f: exit fullscreen  |  ctrl+t: new pane  |  ctrl+w: close pane  |  ctrl+q: quit",
            Style::default().fg(Color::DarkGray),
        )));
    }
    let hint = match app.focus {
        Focus::Terminal => {
            " shift+←/→: cycle  |  ctrl+t: new pane  |  ctrl+w: close pane  |  F1-F9: switch pane  |  ctrl+f: fullscreen  |  ctrl+o: repo  |  ctrl+q: quit"
        }
        Focus::FileList => {
            " shift+←/→: cycle  |  j/k: navigate  |  /: search  |  F1-F9: switch pane  |  ctrl+f: fullscreen  |  ctrl+o: repo  |  ctrl+q: quit"
        }
        Focus::DiffViewer => {
            if app.diff_search_active {
                " type to search  |  enter: confirm  |  esc: cancel"
            } else if !app.diff_search_query.is_empty() {
                " n: next match  |  shift+n: prev match  |  /: new search  |  esc: clear"
            } else {
                " shift+←/→: cycle  |  j/k: scroll  |  /: search  |  pgup/pgdn: scroll  |  F1-F9: switch pane  |  ctrl+f: fullscreen  |  ctrl+o: repo  |  ctrl+q: quit"
            }
        }
    };
    Paragraph::new(Line::from(Span::styled(
        hint,
        Style::default().fg(Color::DarkGray),
    )))
}
