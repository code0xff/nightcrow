mod night;
mod scene;
#[cfg(test)]
mod tests;

pub use night::{TWINKLE_FRAME, draw_idle};

use night::{SCENE_HEIGHT, draw_scene};
use ratatui::{
    Frame,
    layout::{Alignment, Constraint, Direction, Layout},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Paragraph},
};

/// Rows the splash needs under the scene: gap, name, tagline, gap, prompt.
const FOOTER_HEIGHT: u16 = 5;

/// Draw the splash screen: the night scene, which build this is, and how to
/// leave.
///
/// `tick` advances once per twinkle frame and drives the stars; it wraps, so any
/// value is valid. Nothing here dismisses the splash — it stays until the user
/// presses a key (handled by [`crate::application::splash::splash_loop`]).
pub fn draw(frame: &mut Frame, accent: Color, tick: usize) {
    let area = frame.area();

    frame.render_widget(
        Block::default().style(Style::default().bg(Color::Black)),
        area,
    );

    // The scene gives up rows before the text does: a prompt nobody can see is
    // worse than a cropped sky.
    let scene_h = SCENE_HEIGHT.min(area.height.saturating_sub(FOOTER_HEIGHT));

    let outer = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Min(0),
            Constraint::Length(scene_h + FOOTER_HEIGHT),
            Constraint::Min(0),
        ])
        .split(area);

    let inner = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(scene_h), // night scene
            Constraint::Length(1),       // gap
            Constraint::Length(1),       // name + version + commit
            Constraint::Length(1),       // tagline
            Constraint::Length(1),       // gap
            Constraint::Length(1),       // prompt
        ])
        .split(outer[1]);

    draw_scene(frame, inner[0], accent, tick);

    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled(
                "nightcrow",
                Style::default()
                    .fg(Color::White)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                format!("  {}", build_id()),
                Style::default().fg(Color::DarkGray),
            ),
        ]))
        .alignment(Alignment::Center),
        inner[2],
    );

    frame.render_widget(
        Paragraph::new(Line::from(Span::styled(
            "Agent-adjacent TUI",
            Style::default()
                .fg(Color::DarkGray)
                .add_modifier(Modifier::DIM),
        )))
        .alignment(Alignment::Center),
        inner[3],
    );

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

/// Version and the commit it was built from, e.g. `v0.1.1 · 816b8c3f3`. A `+`
/// after the commit means the work tree had uncommitted changes; `unknown`
/// stands in when the build had no git metadata to read (see `build.rs`).
pub(crate) fn build_id() -> String {
    format!(
        "v{} · {}",
        env!("CARGO_PKG_VERSION"),
        env!("NIGHTCROW_COMMIT")
    )
}

/// The build id as its own dim line, for screens that show nothing else.
fn build_line() -> Line<'static> {
    Line::from(Span::styled(
        build_id(),
        Style::default()
            .fg(Color::DarkGray)
            .add_modifier(Modifier::DIM),
    ))
}
