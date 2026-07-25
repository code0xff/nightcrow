use crate::app::App;
use ratatui::{
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::Paragraph,
};

pub(crate) fn render_notice_row<'a>(app: &'a App, accent: Color) -> Paragraph<'a> {
    if let Some(notice) = app.notice.as_ref() {
        return Paragraph::new(Line::from(Span::styled(
            format!(" {}", notice.line()),
            Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
        )));
    }
    render_repo_header(app, accent)
}

pub(crate) fn render_repo_header<'a>(app: &'a App, accent: Color) -> Paragraph<'a> {
    let display_path = home_relative_path(&app.repo_path);
    let mut spans: Vec<Span<'a>> = vec![Span::styled(
        format!(" {display_path} "),
        Style::default()
            .fg(Color::Gray)
            .add_modifier(Modifier::BOLD),
    )];
    if let Some(branch) = app.branch_name.as_deref() {
        spans.push(Span::styled(
            format!(" {branch} "),
            Style::default().fg(accent).add_modifier(Modifier::BOLD),
        ));
    }
    if let Some(t) = &app.tracking
        && (t.ahead > 0 || t.behind > 0)
    {
        spans.push(Span::styled(
            format!(" ↑{} ↓{} ", t.ahead, t.behind),
            Style::default().fg(Color::Cyan),
        ));
    }
    Paragraph::new(Line::from(spans))
}

pub(crate) fn home_relative_path(path: &str) -> String {
    let trimmed = path.trim_end_matches('/');
    if let Some(home) = dirs::home_dir()
        && let Some(home_str) = home.to_str()
        && let Some(rest) = trimmed.strip_prefix(home_str)
    {
        return format!("~{rest}");
    }
    trimmed.to_string()
}
