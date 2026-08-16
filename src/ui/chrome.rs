use crate::config::LayoutConfig;
use crate::ui::status_view::RepoInput;
use ratatui::layout::{Constraint, Direction, Layout, Rect};

pub(crate) fn chrome_rows(screen_area: Rect) -> ChromeRows {
    let outer = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1),
            Constraint::Min(0),
            Constraint::Length(1),
            Constraint::Length(1),
        ])
        .split(screen_area);
    ChromeRows {
        tabs: outer[0],
        body: outer[1],
        notice: outer[2],
        hint: outer[3],
    }
}

pub(crate) struct ChromeRows {
    pub tabs: Rect,
    pub body: Rect,
    pub notice: Rect,
    pub hint: Rect,
}

pub(crate) fn main_content_constraints(layout: &LayoutConfig) -> [Constraint; 2] {
    [
        Constraint::Percentage(layout.upper_pct),
        Constraint::Percentage(100u16.saturating_sub(layout.upper_pct)),
    ]
}

#[derive(Clone, Copy)]
pub struct Chrome<'a> {
    pub repo_paths: &'a [String],
    pub attention: &'a [bool],
    pub attention_bright: bool,
    pub active: usize,
    pub repo_input: &'a RepoInput,
}
