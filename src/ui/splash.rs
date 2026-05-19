use ratatui::{
    Frame,
    layout::{Alignment, Constraint, Direction, Layout},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Paragraph},
};
use std::time::{Duration, Instant};

const LOGO: &[&str] = &[
    "███╗   ██╗██╗ ██████╗ ██╗  ██╗████████╗ ██████╗██████╗  ██████╗ ██╗    ██╗",
    "████╗  ██║██║██╔════╝ ██║  ██║╚══██╔══╝██╔════╝██╔══██╗██╔═══██╗██║    ██║",
    "██╔██╗ ██║██║██║  ███╗███████║   ██║   ██║     ██████╔╝██║   ██║██║ █╗ ██║",
    "██║╚██╗██║██║██║   ██║██╔══██║   ██║   ██║     ██╔══██╗██║   ██║██║███╗██║",
    "██║ ╚████║██║╚██████╔╝██║  ██║   ██║   ╚██████╗██║  ██║╚██████╔╝╚███╔███╔╝",
    "╚═╝  ╚═══╝╚═╝ ╚═════╝ ╚═╝  ╚═╝   ╚═╝    ╚═════╝╚═╝  ╚═╝ ╚═════╝  ╚══╝╚══╝",
];

const SPLASH_DURATION: Duration = Duration::from_millis(1600);
const BAR_WIDTH: usize = 44;

pub struct SplashState {
    start: Instant,
}

impl Default for SplashState {
    fn default() -> Self {
        Self::new()
    }
}

impl SplashState {
    pub fn new() -> Self {
        Self {
            start: Instant::now(),
        }
    }

    pub fn is_done(&self) -> bool {
        self.start.elapsed() >= SPLASH_DURATION
    }

    fn progress(&self) -> f64 {
        (self.start.elapsed().as_secs_f64() / SPLASH_DURATION.as_secs_f64()).min(1.0)
    }
}

pub fn draw(frame: &mut Frame, state: &SplashState, accent: Color) {
    let area = frame.area();

    frame.render_widget(
        Block::default().style(Style::default().bg(Color::Black)),
        area,
    );

    let logo_h = LOGO.len() as u16;
    // logo + gap + tagline + gap + bar
    let content_h = logo_h + 1 + 1 + 1 + 1;

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
            Constraint::Length(logo_h),
            Constraint::Length(1),
            Constraint::Length(1),
            Constraint::Length(1),
            Constraint::Length(1),
        ])
        .split(outer[1]);

    // Logo — brighten as loading completes
    let progress = state.progress();
    let logo_style = if progress < 0.5 {
        Style::default().fg(accent).add_modifier(Modifier::DIM)
    } else {
        Style::default().fg(accent)
    };
    let logo_lines: Vec<Line> = LOGO
        .iter()
        .map(|&row| Line::from(Span::styled(row, logo_style)))
        .collect();
    frame.render_widget(
        Paragraph::new(logo_lines).alignment(Alignment::Center),
        inner[0],
    );

    // Tagline
    let version = env!("CARGO_PKG_VERSION");
    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled(
                "Agent-adjacent TUI",
                Style::default().fg(Color::DarkGray),
            ),
            Span::styled(
                format!("  v{version}"),
                Style::default()
                    .fg(Color::DarkGray)
                    .add_modifier(Modifier::DIM),
            ),
        ]))
        .alignment(Alignment::Center),
        inner[2],
    );

    // Loading bar
    let filled = ((progress * BAR_WIDTH as f64) as usize).min(BAR_WIDTH);
    let empty = BAR_WIDTH - filled;
    let bar = format!("{}{}", "█".repeat(filled), "░".repeat(empty));

    frame.render_widget(
        Paragraph::new(Line::from(Span::styled(bar, Style::default().fg(accent))))
            .alignment(Alignment::Center),
        inner[4],
    );
}
