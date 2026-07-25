use crate::app::App;
use crate::runtime::terminal::visible_range;
use crate::ui::terminal_tab::layout::{
    JUMP_KEY_PANE_COUNT, TAB_TITLE_MAX_CHARS, terminal_layout, truncate_tab_title,
};
use ratatui::{
    Frame,
    layout::{Position, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::Paragraph,
};

/// What one rendered tab-bar segment is, deciding both its style and what a
/// click on it does.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum TabSegment {
    /// The `<leader> t: new terminal` legend shown with no panes. Inert.
    Legend,
    /// A pane's tab; a click jumps to this pane index.
    Tab(usize),
    /// A `+N` hidden-pane marker; a click jumps to the nearest hidden pane
    /// on its side (`visible.start - 1` / `visible.end`), which slides the
    /// visible window by exactly one slot via `sync_visible_window` — the
    /// minimal reveal.
    Marker(usize),
}

impl TabSegment {
    /// The pane index a click on this segment jumps to, if any.
    fn click_target(self) -> Option<usize> {
        match self {
            TabSegment::Legend => None,
            TabSegment::Tab(i) | TabSegment::Marker(i) => Some(i),
        }
    }
}

/// The tab bar's rendered segments in draw order. Single source for
/// `render_tab_bar` (which styles them) and `tab_target_at` (which measures
/// them), so the click hit-test cannot drift from the drawn labels.
pub(crate) fn tab_segments(app: &App, visible: std::ops::Range<usize>) -> Vec<(String, TabSegment)> {
    if app.terminal.panes.is_empty() {
        return vec![(
            format!(" {} t: new terminal ", app.leader_label()),
            TabSegment::Legend,
        )];
    }
    // While the terminal fills the body the upper viewer is hidden, so
    // `<prefix> 1..8` address panes 0..7 directly; label the tabs with those
    // digits. In the split view the digits `1`/`2` belong to the list/diff,
    // so the pane legend stays on `F3..F10` there.
    let fullscreen = app.terminal.fullscreen.fills_body();
    let hidden_before = visible.start;
    let hidden_after = app.terminal.panes.len().saturating_sub(visible.end);
    let mut segments = Vec::new();
    if hidden_before > 0 {
        segments.push((
            format!(" +{hidden_before} "),
            TabSegment::Marker(visible.start - 1),
        ));
    }
    segments.extend(app.terminal.panes[visible.clone()].iter().enumerate().map(
        |(offset, pane)| {
            let i = visible.start + offset;
            // Panes 0..=7 carry a jump key: `<prefix> 1..8` in fullscreen,
            // `<prefix> 3..9,0` in the split view (the digit row is
            // layout-aware). Panes past the 8th have no jump key, so they
            // carry no hint to avoid implying an unbound shortcut. The bare
            // F-keys are NOT advertised here: they select project tabs.
            let title = truncate_tab_title(&pane.title, TAB_TITLE_MAX_CHARS);
            let label = if i < JUMP_KEY_PANE_COUNT {
                // Split view runs 3,4..9 then wraps to 0 for the eighth pane.
                let digit = if fullscreen {
                    char::from_digit(i as u32 + 1, 10).unwrap_or('?')
                } else {
                    char::from_digit((i as u32 + 3) % 10, 10).unwrap_or('?')
                };
                format!(" {}{} {} ", app.leader_label(), digit, title)
            } else {
                format!(" {} ", title)
            };
            (label, TabSegment::Tab(i))
        },
    ));
    if hidden_after > 0 {
        segments.push((
            format!(" +{hidden_after} "),
            TabSegment::Marker(visible.end),
        ));
    }
    segments
}

/// The pane index a click at screen cell `(x, y)` on the tab bar should jump
/// to: a tab targets its own pane, a `+N` marker the nearest hidden pane on
/// its side. `None` off the tab row, past the last segment, or on the
/// no-panes legend. `area` is the full terminal widget Rect, exactly what
/// `render` receives.
pub(crate) fn tab_target_at(app: &App, area: Rect, x: u16, y: u16) -> Option<usize> {
    let (tab_area, _) = terminal_layout(area)?;
    if !tab_area.contains(Position { x, y }) {
        return None;
    }
    let visible = visible_range(
        app.terminal.visible_start,
        app.terminal.active,
        app.terminal.panes.len(),
        app.terminal.max_visible(),
    );
    let mut cursor = tab_area.x;
    for (text, segment) in tab_segments(app, visible) {
        let width = Span::raw(text.as_str()).width() as u16;
        if x >= cursor && x < cursor + width {
            return segment.click_target();
        }
        cursor += width;
    }
    None
}

pub(crate) fn render_tab_bar(
    frame: &mut Frame,
    app: &App,
    tab_area: Rect,
    accent: Color,
    focused: bool,
    visible: std::ops::Range<usize>,
) {
    let tab_spans: Vec<Span> = tab_segments(app, visible)
        .into_iter()
        .map(|(text, segment)| {
            let style = match segment {
                TabSegment::Tab(i) if i == app.terminal.active && focused => Style::default()
                    .fg(Color::Black)
                    .bg(accent)
                    .add_modifier(Modifier::BOLD),
                TabSegment::Tab(_) => Style::default().fg(Color::Gray),
                TabSegment::Legend | TabSegment::Marker(_) => Style::default().fg(Color::DarkGray),
            };
            Span::styled(text, style)
        })
        .collect();
    frame.render_widget(Paragraph::new(Line::from(tab_spans)), tab_area);
}
