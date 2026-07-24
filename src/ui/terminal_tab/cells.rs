use crate::app::App;
use crate::backend::PaneId;
use crate::runtime::terminal::visible_range;
use crate::ui::terminal_tab::layout::{split_pane_areas, terminal_layout};
use ratatui::{
    layout::Rect,
    widgets::{Block, Borders},
};

/// One visible split-view cell: `outer` is the full grid cell (border +
/// content), `content` is where the PTY screen actually draws. For the
/// single-pane case `outer == content` and `bordered` is `false` — no cell
/// border is drawn, matching pre-split-view rendering exactly.
pub(crate) struct VisiblePaneCell {
    pub id: PaneId,
    pub outer: Rect,
    pub content: Rect,
    pub bordered: bool,
}

/// Lay out every currently visible pane inside `content_area` (the terminal
/// body, i.e. below the tab row). This is the single source of truth for
/// pane sizing: `render` draws from it and `visible_pane_content_areas` (used
/// to resize each pane's PTY) reads from it, so a pane's backend/emulator size
/// always matches what's actually drawn on screen.
pub(crate) fn visible_pane_cells(app: &App, content_area: Rect) -> Vec<VisiblePaneCell> {
    let pane_count = app.terminal.panes.len();
    let visible = visible_range(
        app.terminal.visible_start,
        app.terminal.active,
        pane_count,
        app.terminal.max_visible(),
    );
    if visible.is_empty() {
        return Vec::new();
    }
    let visible_ids: Vec<PaneId> = app.terminal.panes[visible].iter().map(|p| p.id).collect();

    if visible_ids.len() == 1 {
        return vec![VisiblePaneCell {
            id: visible_ids[0],
            outer: content_area,
            content: content_area,
            bordered: false,
        }];
    }

    let outers = split_pane_areas(content_area, visible_ids.len());
    visible_ids
        .into_iter()
        .zip(outers)
        .map(|(id, outer)| VisiblePaneCell {
            id,
            outer,
            content: Block::default().borders(Borders::ALL).inner(outer),
            bordered: true,
        })
        .collect()
}

/// Content Rect (post border) for every currently visible pane, keyed by
/// pane id. Used by the main loop to resize each pane's backend PTY and
/// emulator to exactly what `render` draws inside it.
pub(crate) fn visible_pane_content_areas(app: &App, area: Rect) -> Vec<(PaneId, Rect)> {
    let Some((_, content_area)) = terminal_layout(area) else {
        return Vec::new();
    };
    visible_pane_cells(app, content_area)
        .into_iter()
        .map(|cell| (cell.id, cell.content))
        .collect()
}
