mod crow;
#[cfg(test)]
mod tests;

use ratatui::{
    Frame,
    layout::{Alignment, Constraint, Direction, Layout},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Paragraph},
};
use std::time::Duration;

/// How long one wing position is held.
pub const FLAP_FRAME: Duration = Duration::from_millis(110);

/// Draw the splash screen: the flapping crow, version, and a key-press prompt.
///
/// `tick` advances once per animation frame and drives the flap; it wraps, so
/// any value is valid. Nothing here dismisses the splash — it stays until the
/// user presses a key (handled by [`crate::application::splash::splash_loop`]).
pub fn draw(frame: &mut Frame, accent: Color, tick: usize) {
    let area = frame.area();

    frame.render_widget(
        Block::default().style(Style::default().bg(Color::Black)),
        area,
    );

    let version = env!("CARGO_PKG_VERSION");
    let logo_h = crow::HEIGHT as u16;
    // crow + branch + gap + version + gap + prompt
    let content_h = logo_h + 1 + 1 + 1 + 1 + 1;

    let outer = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Min(0),
            Constraint::Length(content_h),
            Constraint::Min(0),
        ])
        .split(area);

    let inner = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(logo_h), // crow
            Constraint::Length(1),      // branch
            Constraint::Length(1),      // gap
            Constraint::Length(1),      // version + subtitle
            Constraint::Length(1),      // gap
            Constraint::Length(1),      // prompt
        ])
        .split(outer[1]);

    // Crow logo. Every row is padded to the same width by `crow::frame`, so
    // Paragraph's per-line centring lands them all on the same column.
    let logo_lines: Vec<Line> = crow::frame(tick)
        .into_iter()
        .map(|row| Line::from(Span::styled(row, Style::default().fg(accent))))
        .collect();
    frame.render_widget(
        Paragraph::new(logo_lines).alignment(Alignment::Center),
        inner[0],
    );

    // Branch under the crow, spanning the logo's width
    frame.render_widget(
        Paragraph::new(Line::from(Span::styled(
            "─".repeat(crow::WIDTH),
            Style::default().fg(accent).add_modifier(Modifier::DIM),
        )))
        .alignment(Alignment::Center),
        inner[1],
    );

    // Version + tagline
    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled(
                "nightcrow",
                Style::default()
                    .fg(Color::White)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                format!("  v{version}"),
                Style::default().fg(Color::DarkGray),
            ),
            Span::styled("  ·  ", Style::default().fg(Color::DarkGray)),
            Span::styled(
                "Agent-adjacent TUI",
                Style::default()
                    .fg(Color::DarkGray)
                    .add_modifier(Modifier::DIM),
            ),
        ]))
        .alignment(Alignment::Center),
        inner[3],
    );

    // Key-press prompt
    frame.render_widget(
        Paragraph::new(Line::from(Span::styled(
            "Press any key to continue",
            Style::default()
                .fg(Color::DarkGray)
                .add_modifier(Modifier::DIM),
        )))
        .alignment(Alignment::Center),
        inner[5],
    );
}
