mod crow;
mod perch;
#[cfg(test)]
mod tests;

pub use perch::{FLAP_FRAME, draw_idle};

use perch::{PERCH_HEIGHT, draw_perch};
use ratatui::{
    Frame,
    layout::{Alignment, Constraint, Direction, Layout},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Paragraph},
};

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
    // crow + branch, gap, version, gap, prompt
    let content_h = PERCH_HEIGHT + 1 + 1 + 1 + 1;

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
            Constraint::Length(PERCH_HEIGHT), // crow + branch
            Constraint::Length(1),            // gap
            Constraint::Length(1),            // version + subtitle
            Constraint::Length(1),            // gap
            Constraint::Length(1),            // prompt
        ])
        .split(outer[1]);

    draw_perch(frame, inner[0], accent, tick);

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
        inner[2],
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
        inner[4],
    );
}
