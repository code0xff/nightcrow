use crate::app::App;
use crate::git::diff::StatusKind;
use ratatui::{
    Frame,
    layout::Rect,
    style::{Color, Modifier, Style},
    widgets::{Block, Borders, List, ListItem, ListState, Paragraph},
};
use std::sync::OnceLock;
use std::time::{Duration, Instant};

pub(crate) fn path_extension(path: &str) -> &str {
    std::path::Path::new(path)
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
}

pub(crate) fn focused_border_style(focused: bool, accent: Color) -> Style {
    if focused {
        Style::default().fg(accent)
    } else {
        Style::default().fg(Color::DarkGray)
    }
}

pub(crate) fn status_color(status: StatusKind) -> Color {
    match status {
        StatusKind::Added => Color::Green,
        StatusKind::Deleted => Color::Red,
        StatusKind::Renamed => Color::Cyan,
        StatusKind::TypeChanged => Color::Magenta,
        StatusKind::Unmerged => Color::Red,
        StatusKind::Untracked => Color::Gray,
        StatusKind::Modified => Color::Yellow,
        StatusKind::Unmodified => Color::DarkGray,
    }
}

/// Space-separated because the leader is a *sequence*, not a chord: `^F1` reads
/// as Ctrl+F1, and that misreading names a real binding — the bare F-keys
/// select project tabs. Matches how the hint bar already writes `^F t`.
pub(crate) fn jump_legend(app: &App, digit: char) -> String {
    format!("{} {}", app.leader_label(), digit)
}

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

/// Full on-off period of the search caret — the Windows console's default.
pub(crate) const CARET_BLINK: Duration = Duration::from_millis(530);

/// Whether the caret is in the lit half of its cycle. Driven by our own clock
/// because `Modifier::SLOW_BLINK` is widely ignored (Windows conhost among
/// them); the event loop's unconditional 16 ms redraw is the frame clock.
pub(crate) fn caret_lit(elapsed: Duration) -> bool {
    (elapsed.as_millis() / (CARET_BLINK.as_millis() / 2)).is_multiple_of(2)
}

/// One origin for all frames, so every caret blinks in step.
fn blink_phase() -> Duration {
    static ORIGIN: OnceLock<Instant> = OnceLock::new();
    ORIGIN.get_or_init(Instant::now).elapsed()
}

pub(crate) fn render_search_bar(
    frame: &mut Frame,
    query: &str,
    is_active: bool,
    area: Rect,
    accent: Color,
) {
    // A blank in the dark half keeps the cell, so the row does not shift.
    let cursor = match (is_active, caret_lit(blink_phase())) {
        (false, _) => "",
        (true, true) => "█",
        (true, false) => " ",
    };
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

pub(crate) fn char_offset(s: &str, scroll_x: usize) -> &str {
    if scroll_x == 0 {
        return s;
    }
    let byte_off = s
        .char_indices()
        .nth(scroll_x)
        .map(|(b, _)| b)
        .unwrap_or(s.len());
    &s[byte_off..]
}
