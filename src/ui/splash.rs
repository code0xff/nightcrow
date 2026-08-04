use ratatui::{
    Frame,
    layout::{Alignment, Constraint, Direction, Layout},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Paragraph},
};

/// A perched crow silhouette — head with a beak and eye on the upper-left,
/// body sweeping down to a pointed tail on the lower-right, sitting on a branch.
const CROW: &[&str] = &[
    "      ▄▄▄",
    "    ▄█▀ ▀█▄",
    "   ▄█  ●  █▄",
    "   █▀▀   ▀▀█▄▄▄",
    "    ██████████████▄",
    "     ███████████████▄",
    "      ████████████████▄",
    "       █████████████████▄",
    "        ██████████████████▄",
    "         ███████████████████",
    "          ████████████████▀",
    "           ██████████████▀",
    "            ████████████▀",
    "             ██████████▀",
    "              ████████▀",
    "               ██████▀",
    "                ████▀",
    "                 ██▀",
];

const BRANCH: &str = "      ───────────────────────────";

/// Draw the splash screen: crow logo, version, and a key-press prompt.
///
/// There is no timer — the splash stays until the user presses a key
/// (handled by [`crate::application::splash::splash_loop`]).
pub fn draw(frame: &mut Frame, accent: Color) {
    let area = frame.area();

    frame.render_widget(
        Block::default().style(Style::default().bg(Color::Black)),
        area,
    );

    let version = env!("CARGO_PKG_VERSION");
    let logo_h = CROW.len() as u16;
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

    // Crow logo
    let logo_lines: Vec<Line> = CROW
        .iter()
        .map(|&row| Line::from(Span::styled(row, Style::default().fg(accent))))
        .collect();
    frame.render_widget(
        Paragraph::new(logo_lines).alignment(Alignment::Center),
        inner[0],
    );

    // Branch under the crow
    frame.render_widget(
        Paragraph::new(Line::from(Span::styled(
            BRANCH,
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
