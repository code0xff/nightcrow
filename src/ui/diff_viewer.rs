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
use syntect::highlighting::ThemeSet;
use syntect::parsing::SyntaxSet;

fn rgb_to_color(rgb: (u8, u8, u8)) -> Color {
    Color::Rgb(rgb.0, rgb.1, rgb.2)
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
            .selected_filtered_status_file()
            .map(|f| f.path.as_str())
            .unwrap_or(""),
    }
}

pub fn render(
    frame: &mut Frame,
    app: &mut App,
    area: Rect,
    ss: &SyntaxSet,
    ts: &ThemeSet,
    accent: ratatui::style::Color,
) {
    if app.diff.view == DiffPaneView::File {
        render_file_view(frame, app, area, ss, ts, accent);
        return;
    }

    let show_search = app.diff.search.is_visible();

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

    let syntax = {
        let file_path = file_path_for_syntax(app);
        ss.find_syntax_by_extension(extension(file_path))
            .unwrap_or_else(|| ss.find_syntax_plain_text())
    };
    // Build the syntect highlight cache once per (hunks × syntax) so the
    // visible-window walk below stays bounded even on large diffs.
    app.diff.ensure_highlight_cache(ss, ts, syntax);

    let current_match = app.diff.search.current_match();
    let has_search = app.diff.search.has_query();

    // Total flat row count = (1 hunk header + N body lines) per hunk.
    let total_lines = app.diff.line_count();
    let visible_height = (diff_area.height as usize).saturating_sub(2);
    let scroll_start = app.diff.scroll.min(app.diff.max_scroll());
    let visible_end = scroll_start.saturating_add(visible_height);

    let mut lines: Vec<Line> = Vec::with_capacity(visible_height);
    let mut flat_idx: usize = 0;

    'outer: for (hi, hunk) in app.diff.hunks.iter().enumerate() {
        if flat_idx >= visible_end {
            break;
        }

        // Hunk header
        if flat_idx >= scroll_start && flat_idx < visible_end {
            lines.push(Line::from(Span::styled(
                hunk.header.as_str(),
                Style::default().fg(Color::Cyan),
            )));
        }
        flat_idx += 1;

        for (li, diff_line) in hunk.lines.iter().enumerate() {
            if flat_idx >= visible_end {
                break 'outer;
            }
            if flat_idx < scroll_start {
                flat_idx += 1;
                continue;
            }

            let is_current = has_search && current_match == Some(flat_idx);
            let is_match = has_search && app.diff.search.is_match(flat_idx);

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

            // Read from the prebuilt highlight cache. Shape is guaranteed to
            // match `hunks` after `ensure_highlight_cache`; treat any
            // mismatch as a fallback path that just renders the raw text.
            if let Some(segs) = app.diff.line_highlights.get(hi).and_then(|hh| hh.get(li)) {
                for seg in segs {
                    spans.push(Span::styled(
                        seg.text.as_str(),
                        Style::default().fg(rgb_to_color(seg.rgb)).bg(bg),
                    ));
                }
            } else {
                spans.push(Span::styled(
                    diff_line.content.as_str(),
                    Style::default().bg(bg),
                ));
            }

            lines.push(Line::from(spans));
            flat_idx += 1;
        }
    }

    if lines.is_empty() && total_lines == 0 {
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
                let count = app.diff.search.matches.len();
                if count == 0 {
                    format!(" F2 {label} [no matches] ")
                } else {
                    format!(" F2 {label} [{}/{}] ", app.diff.search.cursor + 1, count)
                }
            } else {
                format!(" F2 {label} ")
            }
        }
        ViewMode::Status => {
            let selected = app.selected_filtered_status_file();
            if has_search {
                let count = app.diff.search.matches.len();
                let file = selected.map(|f| f.path.as_str()).unwrap_or("Diff");
                if count == 0 {
                    format!(" F2 {file} [no matches] ")
                } else {
                    format!(" F2 {file} [{}/{}] ", app.diff.search.cursor + 1, count)
                }
            } else if let Some(f) = selected {
                format!(" F2 {} ", f.path)
            } else {
                " F2 Diff ".to_string()
            }
        }
    };

    let para = Paragraph::new(lines)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title(title)
                .border_style(border_style),
        )
        .scroll((0, app.diff.scroll_x.min(u16::MAX as usize) as u16));

    frame.render_widget(para, diff_area);

    if let Some(sa) = search_area {
        super::render_search_bar(
            frame,
            &app.diff.search.query,
            app.diff.search.active,
            sa,
            accent,
        );
    }
}

fn render_file_view(
    frame: &mut Frame,
    app: &mut App,
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

    let title = format!(" F2 {file_path} [file] ");

    let visible_height = (area.height as usize).saturating_sub(2);
    let lines: Vec<Line> = if let Some(err) = &app.diff.file_view.error {
        vec![Line::from(Span::styled(
            err.as_str(),
            Style::default().fg(Color::Red),
        ))]
    } else if app.diff.file_view.content.is_empty() {
        vec![Line::from(Span::styled(
            "(empty file)",
            Style::default().fg(Color::DarkGray),
        ))]
    } else {
        app.diff.file_view.ensure_highlight_cache(ss, ts, syntax);
        let fv = &app.diff.file_view;
        let total = fv.line_count();
        let width = total.to_string().len();
        // Belt-and-braces: ensure_highlight_cache keeps line_highlights aligned
        // with content.lines().count(), but if that invariant ever slips the
        // slice below would panic. Clamp against the cache length so a stale
        // total_lines can never produce an out-of-range start.
        let max_scroll = total
            .saturating_sub(1)
            .min(fv.line_highlights.len().saturating_sub(1));
        let scroll_start = fv.scroll.min(max_scroll);
        let scroll_end = scroll_start
            .saturating_add(visible_height)
            .min(fv.line_highlights.len());

        fv.line_highlights[scroll_start..scroll_end]
            .iter()
            .enumerate()
            .map(|(i, segs)| {
                let line_no = scroll_start + i + 1;
                let is_anchor = fv.anchor_line == Some(line_no);
                let bg = if is_anchor {
                    Color::Rgb(60, 60, 90)
                } else {
                    Color::Reset
                };
                let mut spans = vec![Span::styled(
                    format!(" {:>width$} ", line_no, width = width),
                    Style::default().fg(Color::DarkGray).bg(bg),
                )];
                for seg in segs {
                    spans.push(Span::styled(
                        seg.text.as_str(),
                        Style::default().fg(rgb_to_color(seg.rgb)).bg(bg),
                    ));
                }
                Line::from(spans)
            })
            .collect()
    };

    let para = Paragraph::new(lines)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title(title)
                .border_style(border_style),
        )
        .scroll((0, app.diff.file_view.scroll_x.min(u16::MAX as usize) as u16));
    frame.render_widget(para, area);
}
