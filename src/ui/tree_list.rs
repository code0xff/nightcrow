//! Renderer for the read-only file-tree navigator pane (`ViewMode::Tree`).
//! Rows come from `TreeView::visible_rows`, indented by depth, with an
//! expansion marker for directories and shared `char_offset` horizontal
//! scrolling.

use crate::app::{App, Focus};
use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::ListItem,
};

// VS Code-style thin chevrons rather than filled triangles; each marker is
// two columns wide (glyph + space), matching the file marker so names stay
// aligned.
const EXPANDED_MARKER: &str = "⌄ ";
const COLLAPSED_MARKER: &str = "› ";
const FILE_MARKER: &str = "  ";

pub fn render(frame: &mut Frame, app: &App, area: Rect, accent: Color) {
    let focused = app.focus == Focus::FileList;
    let border_style = super::focused_border_style(focused, accent);

    // Reserve a bottom row for the search input whenever the overlay is open
    // or a query is still showing.
    let show_search = app.tree_view.search_active || !app.tree_view.search_query.is_empty();
    let (list_area, search_area) = if show_search {
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Min(0), Constraint::Length(1)])
            .split(area);
        (chunks[0], Some(chunks[1]))
    } else {
        (area, None)
    };

    let rows = app.tree_view.visible_rows();
    let scroll_x = app.tree_view.scroll_x;

    let items: Vec<ListItem> = rows
        .iter()
        .map(|row| {
            let indent = "  ".repeat(row.depth);
            let marker = if row.is_dir {
                if row.expanded {
                    EXPANDED_MARKER
                } else {
                    COLLAPSED_MARKER
                }
            } else {
                FILE_MARKER
            };
            let full = format!("{indent}{marker}{}", row.name);
            let shown = super::char_offset(&full, scroll_x).to_string();
            // Directories take the accent color so the structure reads at a
            // glance; files render in the default foreground.
            let style = if row.is_dir {
                Style::default().fg(accent).add_modifier(Modifier::BOLD)
            } else {
                Style::default()
            };
            ListItem::new(Line::from(Span::styled(shown, style)))
        })
        .collect();

    let title = if app.tree_view.search_filtering() {
        format!(
            " {} Tree ({}/{}) ",
            super::jump_legend(app, '1'),
            app.tree_view.match_count,
            app.tree_view.index.len()
        )
    } else if rows.is_empty() {
        format!(" {} Tree (empty) ", super::jump_legend(app, '1'))
    } else {
        format!(" {} Tree ", super::jump_legend(app, '1'))
    };

    let selected = if rows.is_empty() {
        None
    } else {
        Some(app.tree_view.selected.min(rows.len() - 1))
    };

    super::render_selectable_list(frame, list_area, title, items, selected, border_style);

    if let Some(sa) = search_area {
        super::render_search_bar(
            frame,
            app.tree_view.search_query.as_str(),
            app.tree_view.search_active,
            sa,
            accent,
        );
    }
}
