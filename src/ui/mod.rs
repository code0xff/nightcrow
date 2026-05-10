pub mod commit_list;
pub mod diff_viewer;
pub mod file_list;
pub mod splash;
pub mod terminal_tab;

use crate::app::{App, DiffPaneView, Focus, ViewMode};
use crate::config::LayoutConfig;
use crate::git::diff::ChangeStatus;
use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem, ListState, Paragraph},
};
use syntect::highlighting::ThemeSet;
use syntect::parsing::SyntaxSet;

pub(crate) fn focused_border_style(focused: bool, accent: Color) -> Style {
    if focused {
        Style::default().fg(accent)
    } else {
        Style::default().fg(Color::DarkGray)
    }
}

pub(crate) fn status_color(status: ChangeStatus) -> Color {
    match status {
        ChangeStatus::Added => Color::Green,
        ChangeStatus::Deleted => Color::Red,
        ChangeStatus::Renamed => Color::Cyan,
        ChangeStatus::Untracked => Color::DarkGray,
        ChangeStatus::Modified => Color::Yellow,
    }
}

/// Render a bordered, single-selection list with the project's standard
/// highlight styling. `selected` is clamped to `items.len() - 1` to match
/// the prior call sites' defensive behaviour.
pub(crate) fn render_selectable_list(
    frame: &mut Frame,
    area: Rect,
    title: String,
    items: Vec<ListItem<'_>>,
    selected: Option<usize>,
    border_style: Style,
) {
    let len = items.len();
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
    if len > 0
        && let Some(idx) = selected
    {
        state.select(Some(idx.min(len - 1)));
    }
    frame.render_stateful_widget(list, area, &mut state);
}

pub(crate) fn render_search_bar(
    frame: &mut Frame,
    query: &str,
    is_active: bool,
    area: Rect,
    accent: Color,
) {
    let cursor = if is_active { "█" } else { "" };
    let style = if is_active {
        Style::default().fg(accent)
    } else {
        Style::default().fg(Color::DarkGray)
    };
    frame.render_widget(
        Paragraph::new(format!("/{query}{cursor}")).style(style),
        area,
    );
}

fn main_content_constraints(layout: &LayoutConfig) -> [Constraint; 2] {
    [
        Constraint::Percentage(layout.upper_pct),
        Constraint::Percentage(100u16.saturating_sub(layout.upper_pct)),
    ]
}

pub fn draw(
    frame: &mut Frame,
    app: &mut App,
    ss: &SyntaxSet,
    ts: &ThemeSet,
    layout: &LayoutConfig,
    accent: Color,
) {
    if app.terminal.fullscreen {
        let root = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Min(0), Constraint::Length(1)])
            .split(frame.area());

        terminal_tab::render(frame, app, root[0], accent);
        frame.render_widget(render_hint_bar(app, accent), root[1]);
        return;
    }

    let root = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(0), Constraint::Length(1)])
        .split(frame.area());

    let main = Layout::default()
        .direction(Direction::Vertical)
        .constraints(main_content_constraints(layout))
        .split(root[0]);

    let file_list_pct = layout.file_list_pct;
    let diff_pct = 100u16.saturating_sub(file_list_pct);
    let upper = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage(file_list_pct),
            Constraint::Percentage(diff_pct),
        ])
        .split(main[0]);

    match app.mode {
        ViewMode::Status => file_list::render(frame, app, upper[0], accent),
        ViewMode::Log => commit_list::render(frame, app, upper[0], accent),
    }
    diff_viewer::render(frame, app, upper[1], ss, ts, accent);
    terminal_tab::render(frame, app, main[1], accent);
    frame.render_widget(render_hint_bar(app, accent), root[1]);
}

fn render_hint_bar(app: &App, accent: Color) -> Paragraph<'_> {
    if app.repo_input.active {
        return Paragraph::new(Line::from(vec![
            Span::styled("repo: ", Style::default().fg(accent)),
            Span::raw(app.repo_input.buf.as_str()),
            Span::styled("█", Style::default().fg(accent)),
        ]));
    }
    if let Some(ref msg) = app.status {
        return Paragraph::new(Line::from(msg.as_str())).style(Style::default().fg(Color::Red));
    }
    if app.terminal.fullscreen {
        return Paragraph::new(Line::from(Span::styled(
            " shift+↑/↓: scroll | shift+pgup/dn: page scroll | shift+←/→: cycle pane | ctrl+f: exit fullscreen | ctrl+t: new pane | ctrl+w: close pane | ctrl+q: quit",
            Style::default().fg(Color::DarkGray),
        )));
    }
    let hint = match app.focus {
        Focus::Terminal => {
            " shift+↑/↓: scroll | shift+pgup/dn: page scroll | shift+←/→: cycle | ctrl+t: new pane | ctrl+w: close pane | F1-F9: switch pane | ctrl+f: fullscreen | ctrl+l: log view | ctrl+o: repo | ctrl+p: theme | ctrl+q: quit"
        }
        Focus::FileList => match app.mode {
            ViewMode::Log => {
                if app.log_view.drill_down {
                    " esc: back to commits | j/k: navigate files | shift+←/→: cycle | ctrl+q: quit"
                } else {
                    " ctrl+l: status view | j/k: navigate commits | enter: view files | shift+←/→: cycle | ctrl+q: quit"
                }
            }
            ViewMode::Status => {
                " shift+←/→: cycle | j/k: navigate | /: search | F1-F9: switch pane | ctrl+f: fullscreen | ctrl+l: log view | ctrl+o: repo | ctrl+p: theme | ctrl+q: quit"
            }
        },
        Focus::DiffViewer => {
            if app.diff.view == DiffPaneView::File {
                " v: back to diff | j/k: scroll | pgup/pgdn: page | shift+←/→: cycle | ctrl+q: quit"
            } else if app.diff.search.active {
                " type to search | enter: confirm | esc: cancel"
            } else if !app.diff.search.query.is_empty() {
                " n: next match | shift+n: prev match | /: new search | esc: clear"
            } else {
                " shift+←/→: cycle | j/k: scroll | v: view file | /: search | pgup/pgdn: scroll | F1-F9: switch pane | ctrl+f: fullscreen | ctrl+l: log view | ctrl+o: repo | ctrl+p: theme | ctrl+q: quit"
            }
        }
    };
    Paragraph::new(Line::from(Span::styled(
        hint,
        Style::default().fg(Color::DarkGray),
    )))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn main_content_split_preserves_lower_panel_at_high_upper_ratio() {
        let cfg = LayoutConfig {
            upper_pct: 99,
            file_list_pct: 25,
        };

        assert_eq!(
            main_content_constraints(&cfg),
            [Constraint::Percentage(99), Constraint::Percentage(1)]
        );
    }
}
