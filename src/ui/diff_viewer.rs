use crate::app::{App, DiffPaneView, Focus, ViewMode};
use crate::git::diff::LineKind;
use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph},
};
use std::path::Path;
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

fn file_path_for_syntax(app: &App) -> &str {
    match app.mode {
        ViewMode::Log if app.log_view.drill_down => app
            .log_view
            .commit_files
            .get(app.log_view.file_selected)
            .map(|f| f.path.as_str())
            .unwrap_or(""),
        ViewMode::Log => "",
        ViewMode::Status => app
            .status_view
            .files
            .get(app.status_view.selected)
            .map(|f| f.path.as_str())
            .unwrap_or(""),
    }
}

pub fn render(
    frame: &mut Frame,
    app: &App,
    area: Rect,
    ss: &SyntaxSet,
    ts: &ThemeSet,
    accent: ratatui::style::Color,
) {
    if app.diff_pane_view == DiffPaneView::File {
        render_file_view(frame, app, area, ss, ts, accent);
        return;
    }

    let show_search = app.diff_search.is_visible();

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
    let border_style = super::focused_border_style(focused, accent);

    let file_path = file_path_for_syntax(app);
    let ext = extension(file_path);
    let syntax = ss
        .find_syntax_by_extension(ext)
        .unwrap_or_else(|| ss.find_syntax_plain_text());
    let theme = &ts.themes["base16-ocean.dark"];

    let current_match = app.diff_search.current_match();
    let has_search = app.diff_search.has_query();

    let mut lines: Vec<Line> = Vec::new();
    let mut flat_idx: usize = 0;

    // Two separate highlighters for the whole diff: removed lines must not bleed
    // their syntax state (e.g. an unclosed string) into the added/context lines.
    // Keeping them alive across hunks lets context carry forward correctly for
    // multiline constructs (block comments, string literals) that span hunk boundaries.
    let mut hl_new = HighlightLines::new(syntax, theme);
    let mut hl_old = HighlightLines::new(syntax, theme);

    for hunk in &app.hunks {
        // Hunk header
        lines.push(Line::from(Span::styled(
            hunk.header.clone(),
            Style::default().fg(Color::Cyan),
        )));
        flat_idx += 1;

        for diff_line in &hunk.lines {
            let is_current = has_search && current_match == Some(flat_idx);
            let is_match = has_search && app.diff_search.is_match(flat_idx);

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

            let hl = match diff_line.kind {
                LineKind::Removed => &mut hl_old,
                _ => &mut hl_new,
            };
            let content_with_newline = format!("{}\n", diff_line.content);
            if let Ok(ranges) = hl.highlight_line(&content_with_newline, ss) {
                for (style, text) in ranges {
                    let t = text.trim_end_matches('\n');
                    if t.is_empty() {
                        continue;
                    }
                    let fg = scolor(style.foreground);
                    spans.push(Span::styled(t.to_string(), Style::default().fg(fg).bg(bg)));
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
        let msg = match app.mode {
            ViewMode::Log => {
                if app.log_view.commits.is_empty() {
                    "No commits in repository"
                } else {
                    "No diff for selected commit"
                }
            }
            ViewMode::Status => {
                if app.status_view.files.is_empty() {
                    "No changes in repository"
                } else {
                    "No diff for selected file"
                }
            }
        };
        lines.push(Line::from(Span::styled(
            msg,
            Style::default().fg(Color::DarkGray),
        )));
    }

    let title = match app.mode {
        ViewMode::Log => {
            let label = if app.log_view.diff_title.is_empty() {
                "Diff"
            } else {
                app.log_view.diff_title.as_str()
            };
            if has_search {
                let count = app.diff_search.matches.len();
                if count == 0 {
                    format!(" {label} [no matches] ")
                } else {
                    format!(" {label} [{}/{}] ", app.diff_search.cursor + 1, count)
                }
            } else {
                format!(" {label} ")
            }
        }
        ViewMode::Status => {
            if has_search {
                let count = app.diff_search.matches.len();
                let file = app
                    .status_view
                    .files
                    .get(app.status_view.selected)
                    .map(|f| f.path.as_str())
                    .unwrap_or("Diff");
                if count == 0 {
                    format!(" {file} [no matches] ")
                } else {
                    format!(" {file} [{}/{}] ", app.diff_search.cursor + 1, count)
                }
            } else if let Some(f) = app.status_view.files.get(app.status_view.selected) {
                format!(" {} ", f.path)
            } else {
                " Diff ".to_string()
            }
        }
    };

    let max_scroll = lines.len().saturating_sub(1);
    let scroll_start = app.scroll.min(max_scroll);
    // Slice the visible window instead of relying on Paragraph::scroll (which is limited to u16).
    let visible_height = (diff_area.height as usize).saturating_sub(2);
    let visible_lines: Vec<Line> = lines
        .into_iter()
        .skip(scroll_start)
        .take(visible_height)
        .collect();

    let para = Paragraph::new(visible_lines)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title(title)
                .border_style(border_style),
        )
        .scroll((0, app.diff_scroll_x.min(u16::MAX as usize) as u16));

    frame.render_widget(para, diff_area);

    if let Some(sa) = search_area {
        super::render_search_bar(
            frame,
            &app.diff_search.query,
            app.diff_search.active,
            sa,
            accent,
        );
    }
}

fn render_file_view(
    frame: &mut Frame,
    app: &App,
    area: Rect,
    ss: &SyntaxSet,
    ts: &ThemeSet,
    accent: ratatui::style::Color,
) {
    let focused = app.focus == Focus::DiffViewer;
    let border_style = super::focused_border_style(focused, accent);
    let file_path = file_path_for_syntax(app);
    let ext = extension(file_path);
    let syntax = ss
        .find_syntax_by_extension(ext)
        .unwrap_or_else(|| ss.find_syntax_plain_text());
    let theme = &ts.themes["base16-ocean.dark"];

    let title = format!(" {file_path} [file] ");

    let mut lines: Vec<Line> = Vec::new();
    if let Some(err) = &app.file_view.error {
        lines.push(Line::from(Span::styled(
            err.clone(),
            Style::default().fg(Color::Red),
        )));
    } else if app.file_view.content.is_empty() {
        lines.push(Line::from(Span::styled(
            "(empty file)",
            Style::default().fg(Color::DarkGray),
        )));
    } else {
        let total = app.file_view.line_count();
        let width = total.to_string().len();
        let anchor = app.file_view.anchor_line;
        let mut hl = HighlightLines::new(syntax, theme);
        for (idx, raw_line) in app.file_view.content.lines().enumerate() {
            let line_no = idx + 1;
            let is_anchor = anchor == Some(line_no);
            let bg = if is_anchor {
                Color::Rgb(60, 60, 90)
            } else {
                Color::Reset
            };
            let mut spans = vec![Span::styled(
                format!(" {:>width$} ", line_no, width = width),
                Style::default().fg(Color::DarkGray).bg(bg),
            )];
            let with_nl = format!("{raw_line}\n");
            if let Ok(ranges) = hl.highlight_line(&with_nl, ss) {
                for (style, text) in ranges {
                    let t = text.trim_end_matches('\n');
                    if t.is_empty() {
                        continue;
                    }
                    spans.push(Span::styled(
                        t.to_string(),
                        Style::default().fg(scolor(style.foreground)).bg(bg),
                    ));
                }
            } else {
                spans.push(Span::styled(
                    raw_line.to_string(),
                    Style::default().bg(bg),
                ));
            }
            lines.push(Line::from(spans));
        }
    }

    let max_scroll = lines.len().saturating_sub(1);
    let scroll_start = app.file_view.scroll.min(max_scroll);
    let visible_height = (area.height as usize).saturating_sub(2);
    let visible: Vec<Line> = lines
        .into_iter()
        .skip(scroll_start)
        .take(visible_height)
        .collect();

    let para = Paragraph::new(visible).block(
        Block::default()
            .borders(Borders::ALL)
            .title(title)
            .border_style(border_style),
    );
    frame.render_widget(para, area);
}
