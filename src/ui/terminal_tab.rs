use ratatui::{
    layout::Rect,
    style::{Color, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph},
    Frame,
};

pub fn render(frame: &mut Frame, area: Rect) {
    let placeholder = Paragraph::new(vec![
        Line::from(Span::styled(
            "  Terminal panes — coming in Increment 2",
            Style::default().fg(Color::DarkGray),
        )),
        Line::from(Span::styled(
            "  Multiple LLM CLI sessions will run here",
            Style::default().fg(Color::DarkGray),
        )),
    ])
    .block(
        Block::default()
            .borders(Borders::ALL)
            .title(" Terminals ")
            .border_style(Style::default().fg(Color::DarkGray)),
    );

    frame.render_widget(placeholder, area);
}
