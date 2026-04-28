use crate::app::{App, Focus};
use crate::git::diff::LineKind;
use ratatui::{
    layout::Rect,
    style::{Color, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph},
    Frame,
};
use syntect::easy::HighlightLines;
use syntect::highlighting::{Color as SColor, ThemeSet};
use syntect::parsing::SyntaxSet;
use syntect::util::LinesWithEndings;

fn scolor(c: SColor) -> Color {
    Color::Rgb(c.r, c.g, c.b)
}

fn extension(path: &str) -> &str {
    path.rsplit('.').next().unwrap_or("")
}

pub fn render(frame: &mut Frame, app: &App, area: Rect, ss: &SyntaxSet, ts: &ThemeSet) {
    let focused = app.focus == Focus::DiffViewer;
    let border_style = if focused {
        Style::default().fg(Color::Yellow)
    } else {
        Style::default().fg(Color::DarkGray)
    };

    let file_path = app.files.get(app.selected).map(|f| f.path.as_str()).unwrap_or("");
    let ext = extension(file_path);
    let syntax = ss
        .find_syntax_by_extension(ext)
        .unwrap_or_else(|| ss.find_syntax_plain_text());
    let theme = &ts.themes["base16-ocean.dark"];

    let mut lines: Vec<Line> = Vec::new();

    for hunk in &app.hunks {
        // Hunk header
        lines.push(Line::from(Span::styled(
            hunk.header.clone(),
            Style::default().fg(Color::Cyan),
        )));

        for diff_line in &hunk.lines {
            let (bg, prefix) = match diff_line.kind {
                LineKind::Added => (Color::Rgb(0, 50, 0), "+"),
                LineKind::Removed => (Color::Rgb(50, 0, 0), "-"),
                LineKind::Context => (Color::Reset, " "),
            };

            let mut spans = vec![Span::styled(
                prefix,
                Style::default().fg(Color::DarkGray).bg(bg),
            )];

            // Syntect highlight the content
            let mut hl = HighlightLines::new(syntax, theme);
            let content_with_newline = format!("{}\n", diff_line.content);
            for line_str in LinesWithEndings::from(&content_with_newline) {
                if let Ok(ranges) = hl.highlight_line(line_str, ss) {
                    for (style, text) in ranges {
                        if text.is_empty() {
                            continue;
                        }
                        let fg = scolor(style.foreground);
                        spans.push(Span::styled(
                            text.trim_end_matches('\n').to_string(),
                            Style::default().fg(fg).bg(bg),
                        ));
                    }
                } else {
                    spans.push(Span::styled(
                        diff_line.content.clone(),
                        Style::default().bg(bg),
                    ));
                }
            }

            lines.push(Line::from(spans));
        }
    }

    if lines.is_empty() {
        if app.files.is_empty() {
            lines.push(Line::from(Span::styled(
                "No changes in repository",
                Style::default().fg(Color::DarkGray),
            )));
        } else {
            lines.push(Line::from(Span::styled(
                "Select a file to view diff",
                Style::default().fg(Color::DarkGray),
            )));
        }
    }

    let title = if let Some(f) = app.files.get(app.selected) {
        format!(" {} ", f.path)
    } else {
        " Diff ".to_string()
    };

    // Clamp scroll
    let max_scroll = lines.len().saturating_sub(1);
    let scroll = app.scroll.min(max_scroll) as u16;

    let para = Paragraph::new(lines)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title(title)
                .border_style(border_style),
        )
        .scroll((scroll, 0));

    frame.render_widget(para, area);
}
