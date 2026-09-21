//! The `<prefix> ?` help overlay: every command with its keys, on one screen.
//!
//! Workspace-level like the repo dialog, because it has to open with no
//! project on screen — that is where a new user most needs it.

use crate::ui::help_entries::{HELP_SECTIONS, HelpRow};
use std::sync::atomic::{AtomicU16, Ordering};

use ratatui::{
    Frame,
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph},
};

/// Widest key column across every row, so the descriptions line up.
const KEYS_WIDTH: usize = 22;
/// Room the frame, the padding and the footer take from the body.
const CHROME_ROWS: u16 = 4;
const MAX_WIDTH: u16 = 84;

/// The one closed `HelpOverlay` for frames built without a workspace behind
/// them, which is only ever a test's frame.
#[cfg(test)]
pub static CLOSED_HELP: HelpOverlay = HelpOverlay::closed();

#[derive(Default)]
pub struct HelpOverlay {
    pub open: bool,
    /// First body line drawn. Interior mutability because the render is the
    /// only place that knows how many lines fit, and the clamp it computes has
    /// to stick: otherwise holding `j` past the end would need as many `k`
    /// presses back. Atomic rather than a `Cell` so `CLOSED_HELP` can be the
    /// shared `static` a borrowed field needs.
    scroll: AtomicU16,
}

impl Clone for HelpOverlay {
    fn clone(&self) -> Self {
        Self {
            open: self.open,
            scroll: AtomicU16::new(self.scroll_offset()),
        }
    }
}

impl HelpOverlay {
    #[cfg(test)]
    const fn closed() -> Self {
        Self {
            open: false,
            scroll: AtomicU16::new(0),
        }
    }

    pub fn toggle(&mut self) {
        self.open = !self.open;
        self.set_scroll(0);
    }

    pub fn close(&mut self) {
        self.open = false;
        self.set_scroll(0);
    }

    pub fn scroll_by(&mut self, lines: i16) {
        self.set_scroll(self.scroll_offset().saturating_add_signed(lines));
    }

    pub fn scroll_offset(&self) -> u16 {
        self.scroll.load(Ordering::Relaxed)
    }

    fn set_scroll(&self, line: u16) {
        self.scroll.store(line, Ordering::Relaxed);
    }
}

/// Render the overlay over whatever is underneath, and clamp the scroll to
/// what fits: the last line must stay reachable and the view must not run
/// past it, and only the render knows the height.
pub(crate) fn render(
    frame: &mut Frame,
    overlay: &HelpOverlay,
    leader_label: &str,
    area: Rect,
    accent: Color,
) {
    let popup = centered(area);
    if popup.height <= CHROME_ROWS || popup.width < 20 {
        return;
    }
    let body_rows = popup.height - CHROME_ROWS;
    let lines = body_lines(leader_label, popup.width);
    let max_scroll = (lines.len() as u16).saturating_sub(body_rows);
    let scroll = overlay.scroll_offset().min(max_scroll);
    overlay.set_scroll(scroll);

    let title = format!(" Keyboard help — {leader_label} ? ");
    let more = if max_scroll == 0 {
        String::new()
    } else {
        format!(" · {}/{}", scroll + 1, max_scroll + 1)
    };
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(accent))
        .title(Span::styled(
            title,
            Style::default().fg(accent).add_modifier(Modifier::BOLD),
        ))
        .title_bottom(Span::styled(
            format!(" j/k scroll · esc close{more} "),
            Style::default().fg(Color::DarkGray),
        ));

    frame.render_widget(Clear, popup);
    frame.render_widget(
        Paragraph::new(lines).block(block).scroll((scroll, 0)),
        popup,
    );
}

/// The popup rect: as wide as the text wants up to `MAX_WIDTH`, and one row
/// short of the screen on each side so what is underneath still frames it.
fn centered(area: Rect) -> Rect {
    let width = MAX_WIDTH.min(area.width.saturating_sub(4));
    let height = area.height.saturating_sub(2);
    Rect {
        x: area.x + (area.width.saturating_sub(width)) / 2,
        y: area.y + (area.height.saturating_sub(height)) / 2,
        width,
        height,
    }
}

fn body_lines(leader_label: &str, width: u16) -> Vec<Line<'static>> {
    let what_width = (width as usize).saturating_sub(KEYS_WIDTH + 4);
    let mut lines = Vec::new();
    for (index, section) in HELP_SECTIONS.iter().enumerate() {
        if index > 0 {
            lines.push(Line::from(""));
        }
        lines.push(Line::from(Span::styled(
            section.title.to_string(),
            Style::default()
                .fg(Color::White)
                .add_modifier(Modifier::BOLD),
        )));
        for row in section.rows {
            lines.extend(row_lines(row, leader_label, what_width));
        }
    }
    lines
}

/// One row, wrapped by hand rather than by `Paragraph::wrap` so the wrapped
/// remainder keeps the key column's indent instead of starting at column zero.
fn row_lines(row: &HelpRow, leader_label: &str, what_width: usize) -> Vec<Line<'static>> {
    let keys = substitute(row.keys, leader_label);
    let what = substitute(row.what, leader_label);
    let mut out = Vec::new();
    for (index, chunk) in wrap(&what, what_width.max(1)).into_iter().enumerate() {
        let key_cell = if index == 0 {
            keys.clone()
        } else {
            String::new()
        };
        out.push(Line::from(vec![
            Span::raw("  "),
            Span::styled(
                format!("{key_cell:KEYS_WIDTH$}"),
                Style::default().fg(Color::Cyan),
            ),
            Span::styled(chunk, Style::default().fg(Color::Gray)),
        ]));
    }
    out
}

/// `{L}` is the leader placeholder; a rebound leader must be what is printed.
fn substitute(text: &str, leader_label: &str) -> String {
    text.replace("{L}", leader_label)
}

fn wrap(text: &str, width: usize) -> Vec<String> {
    let mut lines = Vec::new();
    let mut current = String::new();
    for word in text.split_whitespace() {
        let extra = if current.is_empty() { 0 } else { 1 };
        if !current.is_empty() && current.chars().count() + extra + word.chars().count() > width {
            lines.push(std::mem::take(&mut current));
        }
        if !current.is_empty() {
            current.push(' ');
        }
        current.push_str(word);
    }
    if !current.is_empty() || lines.is_empty() {
        lines.push(current);
    }
    lines
}

#[cfg(test)]
#[path = "help_tests.rs"]
mod tests;
