use crate::config::{LayoutConfig, TabStrip};
use crate::ui::project_tab::STRIP_WIDTH;
use crate::ui::status_view::RepoInput;
use ratatui::layout::{Constraint, Direction, Layout, Rect};

/// The four chrome areas, the single source every renderer, resize and
/// hit-test reads. The notice and hint rows always sit under the body at
/// full width; where the project tabs go is `[layout] tabs`, and the body is
/// whatever is left — a row shorter under a top strip, a column narrower
/// beside a left one. Every area always exists, so nothing the chrome does
/// inserts or removes a row and resizes the PTYs by accident.
pub(crate) fn chrome_areas(screen_area: Rect, strip: TabStrip) -> ChromeAreas {
    let rows = bottom_rows(screen_area);
    let (tabs, body) = match strip {
        TabStrip::Top => {
            let split = Layout::default()
                .direction(Direction::Vertical)
                .constraints([Constraint::Length(1), Constraint::Min(0)])
                .split(rows.above);
            (split[0], split[1])
        }
        TabStrip::Left => {
            let split = Layout::default()
                .direction(Direction::Horizontal)
                .constraints([Constraint::Length(STRIP_WIDTH), Constraint::Min(0)])
                .split(rows.above);
            (split[0], split[1])
        }
    };
    ChromeAreas {
        tabs,
        body,
        notice: rows.notice,
        hint: rows.hint,
    }
}

pub(crate) struct ChromeAreas {
    pub tabs: Rect,
    pub body: Rect,
    pub notice: Rect,
    pub hint: Rect,
}

/// The two rows under the body, and everything above them. The same wherever
/// the tabs are, so the notice row and the hint bar — which never move — can
/// find themselves without being told the layout.
pub(crate) fn bottom_rows(screen_area: Rect) -> BottomRows {
    let outer = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Min(0),
            Constraint::Length(1),
            Constraint::Length(1),
        ])
        .split(screen_area);
    BottomRows {
        above: outer[0],
        notice: outer[1],
        hint: outer[2],
    }
}

pub(crate) struct BottomRows {
    /// The tabs and the body together, however they are divided.
    pub above: Rect,
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
    /// `[layout] tabs`, carried with the chrome so it can be drawn and
    /// hit-tested without the layout — the empty screen has none to hand over.
    pub strip: TabStrip,
}
