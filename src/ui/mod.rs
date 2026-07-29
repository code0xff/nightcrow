pub mod commit_list;
pub mod diff_pane;
pub mod diff_viewer;
pub mod file_list;
pub mod file_view;
pub mod log_view;
pub mod project_tab;
pub mod search;
pub mod splash;
pub mod status_view;
pub mod terminal_tab;
pub mod tree_list;
pub mod tree_view;

pub use search::SearchQuery;

mod chrome;
mod helpers;
mod hint_bar;
mod hint_text;
mod hit_test;
mod notice;
#[cfg(test)]
mod tests;

pub(crate) use chrome::{Chrome, chrome_rows, main_content_constraints};
pub(crate) use helpers::{
    char_offset, focused_border_style, jump_legend, path_extension, render_search_bar,
    render_selectable_list, status_color,
};
pub(crate) use hint_bar::{
    HintClick, empty_hint_click_at, hint_click_at, hint_spans, render_hint_bar,
};
pub(crate) use hint_text::{EMPTY_HINT, EMPTY_HINT_ARMED, PREFIX_CHIP};
pub(crate) use hit_test::{
    pane_at, project_tab_at, tab_click_at, terminal_content_areas, upper_panel_at,
};
#[cfg(test)]
pub(crate) use notice::home_relative_path;
pub(crate) use notice::render_notice_row;

use crate::app::{App, ViewMode};
use crate::config::LayoutConfig;
use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Position},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph},
};
use syntect::highlighting::ThemeSet;
use syntect::parsing::SyntaxSet;

pub fn draw_empty(
    frame: &mut Frame,
    chrome: Chrome<'_>,
    notice: Option<&crate::app::Notice>,
    leader: crossterm::event::KeyEvent,
    prefix_armed: bool,
    mouse_enabled: bool,
    accent: Color,
) {
    let rows = chrome_rows(frame.area());
    frame.render_widget(
        project_tab::render(chrome.repo_paths, chrome.active, rows.tabs, accent),
        rows.tabs,
    );

    let leader_label = crate::app::leader_label_of(leader);
    frame.render_widget(
        Paragraph::new(Line::from(vec![Span::styled(
            format!("  no project open — {leader_label} o to open a repo"),
            Style::default().fg(Color::DarkGray),
        )]))
        .block(Block::default().borders(Borders::ALL)),
        rows.body,
    );

    // Shares `render_notice_row`'s priority order so a notice looks the same
    // wherever it lands; with no project there is no repo header to fall back
    // to, so the row just goes empty.
    let notice_line = notice::notice_or_candidates(notice, chrome.repo_input, rows.notice.width)
        .unwrap_or_default();
    frame.render_widget(Paragraph::new(notice_line), rows.notice);

    // The armed prefix shows the same chip as the project screen: pressing
    // the leader here has to look like it did something, or it reads as a
    // dead key.
    let hint = if chrome.repo_input.active {
        Line::from(vec![
            Span::styled("repo: ", Style::default().fg(accent)),
            Span::raw(chrome.repo_input.buf.clone()),
            Span::styled("█", Style::default().fg(accent)),
        ])
    } else if prefix_armed {
        let mut spans = vec![Span::styled(
            PREFIX_CHIP,
            Style::default()
                .fg(Color::Black)
                .bg(accent)
                .add_modifier(Modifier::BOLD),
        )];
        spans.extend(hint_spans(EMPTY_HINT_ARMED, &leader_label, mouse_enabled));
        Line::from(spans)
    } else {
        Line::from(hint_spans(EMPTY_HINT, &leader_label, mouse_enabled))
    };
    frame.render_widget(Paragraph::new(hint), rows.hint);
}

pub fn draw(
    frame: &mut Frame,
    app: &mut App,
    tabs: Chrome<'_>,
    ss: &SyntaxSet,
    ts: &ThemeSet,
    layout: &LayoutConfig,
    accent: Color,
) -> Option<Position> {
    // Chrome: the project tab row on top, the notice row (repo identity, or a
    // notice covering it) and the hint bar below. The tab row and notice row
    // are rendered here, before any layout branch, so neither is lost to a
    // fullscreen view mode — a tab row that vanished in fullscreen would
    // strand the user with no indication of which project they are in.
    let rows = chrome_rows(frame.area());
    let (body_area, notice_area, hint_area) = (rows.body, rows.notice, rows.hint);

    frame.render_widget(
        project_tab::render(tabs.repo_paths, tabs.active, rows.tabs, accent),
        rows.tabs,
    );
    frame.render_widget(
        render_notice_row(app, tabs.repo_input, accent, notice_area.width),
        notice_area,
    );

    if app.terminal.fullscreen.fills_body() {
        let cursor = terminal_tab::render(frame, app, body_area, accent);
        frame.render_widget(render_hint_bar(app, tabs, accent), hint_area);
        return cursor;
    }

    if app.diff.fullscreen {
        diff_viewer::render(frame, app, body_area, ss, ts, accent);
        frame.render_widget(render_hint_bar(app, tabs, accent), hint_area);
        return None;
    }

    if app.list_fullscreen {
        match app.mode {
            ViewMode::Status => file_list::render(frame, app, body_area, accent),
            ViewMode::Log => commit_list::render(frame, app, body_area, accent),
            ViewMode::Tree => tree_list::render(frame, app, body_area, accent),
        }
        frame.render_widget(render_hint_bar(app, tabs, accent), hint_area);
        return None;
    }

    let main = Layout::default()
        .direction(Direction::Vertical)
        .constraints(main_content_constraints(layout))
        .split(body_area);

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
        ViewMode::Tree => tree_list::render(frame, app, upper[0], accent),
    }
    diff_viewer::render(frame, app, upper[1], ss, ts, accent);
    let cursor = terminal_tab::render(frame, app, main[1], accent);
    frame.render_widget(render_hint_bar(app, tabs, accent), hint_area);
    cursor
}
