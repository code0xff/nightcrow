//! The repo dialog's directory browser, drawn over the whole body rather
//! than floating: every surface in this crate takes a layout area, and mouse
//! capture is on by default, so an overlay would need a hit region of its own.

use crate::ui::render_selectable_list;
use crate::workspace::PathTree;
use ratatui::{
    Frame,
    layout::Rect,
    style::{Color, Style},
    text::{Line, Span},
    widgets::ListItem,
};

/// Two columns per level: enough to read the nesting without pushing long names
/// off the right edge of a deep tree.
const INDENT: usize = 2;

pub(crate) fn render(frame: &mut Frame, tree: &PathTree, area: Rect, accent: Color) {
    let dim = Style::default().fg(Color::DarkGray);
    let (items, selected) = if tree.rows().is_empty() {
        // Nothing selectable, but the box must say why it is blank — an empty
        // frame reads as a failure to load. Enter still picks the root itself.
        (
            vec![ListItem::new(Line::from(Span::styled(
                "  (no sub-directories)",
                dim,
            )))],
            None,
        )
    } else {
        let items = tree
            .rows()
            .iter()
            .map(|row| {
                let marker = if row.expanded { "▾" } else { "▸" };
                ListItem::new(Line::from(vec![
                    Span::raw(" ".repeat(row.depth * INDENT)),
                    Span::styled(format!("{marker} "), dim),
                    Span::raw(row.name.clone()),
                ]))
            })
            .collect();
        (items, Some(tree.selected()))
    };

    render_selectable_list(
        frame,
        area,
        format!(" browse {} ", tree.root_label()),
        items,
        selected,
        Style::default().fg(accent),
    );
}
