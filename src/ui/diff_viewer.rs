use crate::app::{App, Focus};
use crate::git::diff::LineKind;
use std::path::Path;
use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph},
};
use syntect::easy::HighlightLines;
use syntect::highlighting::{Color as SColor, ThemeSet};
use syntect::parsing::SyntaxSet;

fn scolor(c: SColor) -> Color {
    Color::Rgb(c.r, c.g, c.b)
}

fn extension(path: &str) -> &str {
    Path::new(path)
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
}

pub fn render(frame: &mut Frame, app: &App, area: Rect, ss: &SyntaxSet, ts: &ThemeSet) {
    let show_search = app.diff_search_active || !app.diff_search_query.is_empty();

    let (diff_area, search_area) = if show_search {
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Min(0), Constraint::Length(1)])
            .split(area);
        (chunks[0], Some(chunks[1]))
    } else {
        (area, None)
    };

    let focused = app.focus == Focus::DiffViewer;
    let border_style = if focused {
        Style::default().fg(Color::Yellow)
    } else {
        Style::default().fg(Color::DarkGray)
    };

    let file_path = app
        .files
        .get(app.selected)
        .map(|f| f.path.as_str())
        .unwrap_or("");
    let ext = extension(file_path);
    let syntax = ss
        .find_syntax_by_extension(ext)
        .unwrap_or_else(|| ss.find_syntax_plain_text());
    let theme = &ts.themes["base16-ocean.dark"];

    let current_match = app.diff_search_matches.get(app.diff_search_cursor).copied();
    let has_search = !app.diff_search_query.is_empty();

    let mut lines: Vec<Line> = Vec::new();
    let mut flat_idx: usize = 0;

    for hunk in &app.hunks {
        // Hunk header
        lines.push(Line::from(Span::styled(
            hunk.header.clone(),
            Style::default().fg(Color::Cyan),
        )));
        flat_idx += 1;

        // Create once per hunk so syntax state carries across lines within the hunk.
        let mut hl = HighlightLines::new(syntax, theme);
        for diff_line in &hunk.lines {
            let is_current = has_search && current_match == Some(flat_idx);
            let is_match = has_search
                && app
                    .diff_search_matches
                    .binary_search(&flat_idx)
                    .is_ok();

            let bg = if is_current {
                Color::Rgb(100, 80, 0)
            } else if is_match {
                Color::Rgb(50, 42, 0)
            } else {
                match diff_line.kind {
                    LineKind::Added => Color::Rgb(0, 50, 0),
                    LineKind::Removed => Color::Rgb(50, 0, 0),
                    LineKind::Context => Color::Reset,
                }
            };

            let prefix = match diff_line.kind {
                LineKind::Added => "+",
                LineKind::Removed => "-",
                LineKind::Context => " ",
            };

            let mut spans = vec![Span::styled(
                prefix,
                Style::default().fg(Color::DarkGray).bg(bg),
            )];

            let content_with_newline = format!("{}\n", diff_line.content);
            if let Ok(ranges) = hl.highlight_line(&content_with_newline, ss) {
                for (style, text) in ranges {
                    let t = text.trim_end_matches('\n');
                    if t.is_empty() {
                        continue;
                    }
                    let fg = scolor(style.foreground);
                    spans.push(Span::styled(
                        t.to_string(),
                        Style::default().fg(fg).bg(bg),
                    ));
                }
            } else {
                spans.push(Span::styled(
                    diff_line.content.clone(),
                    Style::default().bg(bg),
                ));
            }

            lines.push(Line::from(spans));
            flat_idx += 1;
        }
    }

    if lines.is_empty() {
        let msg = if app.files.is_empty() {
            "No changes in repository"
        } else {
            "No diff for selected file"
        };
        lines.push(Line::from(Span::styled(
            msg,
            Style::default().fg(Color::DarkGray),
        )));
    }

    let title = if has_search {
        let count = app.diff_search_matches.len();
        let file = app.files.get(app.selected).map(|f| f.path.as_str()).unwrap_or("Diff");
        if count == 0 {
            format!(" {} [no matches] ", file)
        } else {
            format!(" {} [{}/{}] ", file, app.diff_search_cursor + 1, count)
        }
    } else if let Some(f) = app.files.get(app.selected) {
        format!(" {} ", f.path)
    } else {
        " Diff ".to_string()
    };

    // Clamp scroll
    let max_scroll = lines.len().saturating_sub(1);
    let scroll = app.scroll.min(max_scroll).min(u16::MAX as usize) as u16;

    let para = Paragraph::new(lines)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title(title)
                .border_style(border_style),
        )
        .scroll((scroll, 0));

    frame.render_widget(para, diff_area);

    if let Some(sa) = search_area {
        let cursor = if app.diff_search_active { "█" } else { "" };
        let search_style = if app.diff_search_active {
            Style::default().fg(Color::Yellow)
        } else {
            Style::default().fg(Color::DarkGray)
        };
        frame.render_widget(
            Paragraph::new(format!("/{}{}", app.diff_search_query, cursor))
                .style(search_style),
            sa,
        );
    }
}
