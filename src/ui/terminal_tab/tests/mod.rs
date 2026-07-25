mod layout_tests;
mod render_tests;
mod tab_tests;

use crate::app::App;
use crate::runtime::terminal::visible_range;
use crate::ui::terminal_tab::layout::terminal_layout;
use crate::ui::terminal_tab::tab_bar::tab_segments;
use ratatui::layout::Rect;
use ratatui::text::Span;

pub(super) fn buffer_text(buf: &ratatui::buffer::Buffer) -> String {
    buf.content.iter().map(|c| c.symbol()).collect()
}

pub(super) fn tab_segment_x(app: &App, area: Rect, nth: usize) -> u16 {
    let (tab_area, _) = terminal_layout(area).unwrap();
    let visible = visible_range(
        app.terminal.visible_start,
        app.terminal.active,
        app.terminal.panes.len(),
        app.terminal.max_visible(),
    );
    let segments = tab_segments(app, visible);
    assert!(nth < segments.len(), "segment {nth} must exist");
    tab_area.x
        + segments
            .iter()
            .take(nth)
            .map(|(text, _)| Span::raw(text.as_str()).width() as u16)
            .sum::<u16>()
}
