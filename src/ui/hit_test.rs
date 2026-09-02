use crate::app::{App, Focus};
use crate::config::LayoutConfig;
use crate::ui::chrome::{Chrome, chrome_areas, main_content_constraints};
use ratatui::layout::{Constraint, Direction, Layout, Position, Rect};

pub(crate) fn project_tab_at(tabs: Chrome<'_>, screen_area: Rect, x: u16, y: u16) -> Option<usize> {
    crate::ui::project_tab::tab_at(
        tabs.repo_paths,
        tabs.attention,
        tabs.active,
        chrome_areas(screen_area, tabs.strip).tabs,
        x,
        y,
        tabs.strip,
    )
}

pub(crate) fn terminal_content_areas(
    app: &App,
    screen_area: Rect,
    layout: &LayoutConfig,
) -> Vec<(crate::backend::PaneId, Rect)> {
    let Some(widget_area) = terminal_widget_area(app, screen_area, layout) else {
        return Vec::new();
    };
    crate::ui::terminal_tab::visible_pane_content_areas(app, widget_area)
}

pub(crate) fn upper_panel_at(
    app: &App,
    screen_area: Rect,
    layout: &LayoutConfig,
    x: u16,
    y: u16,
) -> Option<Focus> {
    if app.terminal.fullscreen.fills_body() || app.diff_pane().fullscreen || app.list_fullscreen {
        return None;
    }
    let main = Layout::default()
        .direction(Direction::Vertical)
        .constraints(main_content_constraints(layout))
        .split(chrome_areas(screen_area, layout.tabs).body);
    let file_list_pct = layout.file_list_pct;
    let upper = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage(file_list_pct),
            Constraint::Percentage(100u16.saturating_sub(file_list_pct)),
        ])
        .split(main[0]);

    let pos = Position { x, y };
    if upper[0].contains(pos) {
        Some(Focus::FileList)
    } else if upper[1].contains(pos) {
        Some(Focus::DiffViewer)
    } else {
        None
    }
}

pub(crate) fn pane_at(
    app: &App,
    screen_area: Rect,
    layout: &LayoutConfig,
    x: u16,
    y: u16,
) -> Option<(crate::backend::PaneId, Rect)> {
    terminal_content_areas(app, screen_area, layout)
        .into_iter()
        .find(|(_, rect)| rect.contains(Position { x, y }))
}

pub(crate) fn terminal_widget_area(
    app: &App,
    screen_area: Rect,
    layout: &LayoutConfig,
) -> Option<Rect> {
    let body_area = chrome_areas(screen_area, layout.tabs).body;

    if app.terminal.fullscreen.fills_body() {
        return Some(body_area);
    }
    if app.diff_pane().fullscreen || app.list_fullscreen {
        return None;
    }

    let main = Layout::default()
        .direction(Direction::Vertical)
        .constraints(main_content_constraints(layout))
        .split(body_area);
    Some(main[1])
}

pub(crate) fn tab_click_at(
    app: &App,
    screen_area: Rect,
    layout: &LayoutConfig,
    x: u16,
    y: u16,
) -> Option<usize> {
    let widget_area = terminal_widget_area(app, screen_area, layout)?;
    crate::ui::terminal_tab::tab_target_at(app, widget_area, x, y)
}
