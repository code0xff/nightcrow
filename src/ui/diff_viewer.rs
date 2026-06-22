use crate::app::{App, DiffPaneView, Focus, ViewMode};
use crate::git::diff::LineKind;
use crate::ui::diff_pane::SplitRow;
use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph},
};
use syntect::highlighting::ThemeSet;
use syntect::parsing::SyntaxSet;

/// Minimum pane width (columns) for the side-by-side split layout. Below this
/// each half is too narrow to read, so `Split` view transparently falls back
/// to the unified diff renderer.
const MIN_SPLIT_WIDTH: u16 = 80;

fn rgb_to_color(rgb: (u8, u8, u8)) -> Color {
    Color::Rgb(rgb.0, rgb.1, rgb.2)
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

    // Render side-by-side only when there is a diff to split and the pane is
    // wide enough; otherwise fall through to the unified renderer below.
    if app.diff.view == DiffPaneView::Split
        && area.width >= MIN_SPLIT_WIDTH
        && !app.diff.hunks.is_empty()
    {
        render_split_view(frame, app, area, ss, ts, accent);
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

    // Build the syntect highlight cache once per (hunks × per-hunk syntax)
    // so the visible-window walk below stays bounded even on large diffs.
    // Each hunk carries its own file_path now, so commit diffs that touch
    // multiple file types stop rendering as plain text.
    app.diff.ensure_highlight_cache(ss, ts);

    let current_match = app.diff.search.current_match();
    let has_search = app.diff.search.has_query();

    // Total flat row count = (1 hunk header + N body lines) per hunk.
    let total_lines = app.diff.line_count();
    let visible_height = (diff_area.height as usize).saturating_sub(2);
    let scroll_start = app.diff.scroll.min(app.diff.max_scroll());
    // Keep the stored cursor in sync with the clamped value so a Split-view
    // scroll position that overshoots this (narrower) unified fallback layout
    // is corrected on the frame it falls back.
    app.diff.scroll = scroll_start;
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
            // Tree mode renders the file overlay, not the unified diff, so this
            // message is only reachable if the diff view is forced open with no
            // file selected.
            ViewMode::Tree => "Select a file to preview",
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
        ViewMode::Tree => {
            let path = app.tree_view.selected_path();
            let label = path.as_deref().unwrap_or("File");
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
            app.diff.search.query.as_str(),
            app.diff.search.active,
            sa,
            accent,
        );
    }
}

/// Render the loaded diff as two vertically-scrolled columns (old | new).
/// Reuses the syntect highlight cache and the `scroll`/`scroll_x` cursors so
/// it stays bounded on large diffs. Search highlighting is intentionally not
/// drawn here — match rows are indexed against the unified flat-row model.
fn render_split_view(
    frame: &mut Frame,
    app: &mut App,
    area: Rect,
    ss: &SyntaxSet,
    ts: &ThemeSet,
    accent: ratatui::style::Color,
) {
    let focused = app.focus == Focus::DiffViewer;
    let border_style = super::focused_border_style(focused, accent);
    app.diff.ensure_highlight_cache(ss, ts);

    let rows = app.diff.split_rows();
    let visible_height = (area.height as usize).saturating_sub(2);
    let max_scroll = rows.len().saturating_sub(1);
    let scroll_start = app.diff.scroll.min(max_scroll);
    // Pin the shared scroll cursor to what this layout can actually show. The
    // split layout is shorter than the unified flat-row count (paired changes
    // collapse onto one row), and navigation clamps against the unified max —
    // writing the clamped value back keeps `k`/pgup responsive immediately
    // after bottoming out instead of unwinding phantom rows.
    app.diff.scroll = scroll_start;
    let scroll_end = scroll_start.saturating_add(visible_height).min(rows.len());

    let mut left_lines: Vec<Line> = Vec::with_capacity(visible_height);
    let mut right_lines: Vec<Line> = Vec::with_capacity(visible_height);
    for row in &rows[scroll_start..scroll_end] {
        match row {
            SplitRow::Header(hi) => {
                let header = app
                    .diff
                    .hunks
                    .get(*hi)
                    .map(|h| h.header.as_str())
                    .unwrap_or("");
                left_lines.push(Line::from(Span::styled(
                    header,
                    Style::default().fg(Color::Cyan),
                )));
                right_lines.push(Line::from(""));
            }
            SplitRow::Body { left, right } => {
                left_lines.push(split_side_line(app, *left));
                right_lines.push(split_side_line(app, *right));
            }
        }
    }

    let title = split_title(app);
    let block = Block::default()
        .borders(Borders::ALL)
        .title(title)
        .border_style(border_style);
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let halves = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
        .split(inner);

    let scroll_x = app.diff.scroll_x.min(u16::MAX as usize) as u16;
    let left_para = Paragraph::new(left_lines).scroll((0, scroll_x));
    // A left border on the right column draws the vertical divider between the
    // two halves and indents the new-side content by one cell.
    let right_para = Paragraph::new(right_lines)
        .block(
            Block::default()
                .borders(Borders::LEFT)
                .border_style(border_style),
        )
        .scroll((0, scroll_x));

    frame.render_widget(left_para, halves[0]);
    frame.render_widget(right_para, halves[1]);
}

/// Build one side's `Line` for a split body row. `None` (no counterpart line
/// on this side) renders as a blank line; otherwise the cell is styled by line
/// kind and reuses the prebuilt highlight cache, mirroring the unified
/// renderer's per-line treatment.
fn split_side_line<'a>(app: &'a App, cell: Option<(usize, usize)>) -> Line<'a> {
    let Some((hi, li)) = cell else {
        return Line::from("");
    };
    let Some(diff_line) = app.diff.hunks.get(hi).and_then(|h| h.lines.get(li)) else {
        return Line::from("");
    };

    let bg = match diff_line.kind {
        LineKind::Added => Color::Rgb(0, 50, 0),
        LineKind::Removed => Color::Rgb(50, 0, 0),
        LineKind::Context => Color::Reset,
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
    Line::from(spans)
}

/// Title for the split pane: the same file/commit label the unified view uses,
/// tagged `[split]`. Search match counts are omitted because the split view
/// does not render search highlights.
fn split_title(app: &App) -> String {
    let label = match app.mode {
        ViewMode::Log => {
            if app.log_view.diff_title.is_empty() {
                "Diff".to_string()
            } else {
                app.log_view.diff_title.clone()
            }
        }
        ViewMode::Status => app
            .selected_filtered_status_file()
            .map(|f| f.path.clone())
            .unwrap_or_else(|| "Diff".to_string()),
        ViewMode::Tree => app
            .tree_view
            .selected_path()
            .unwrap_or_else(|| "File".to_string()),
    };
    format!(" F2 {label} [split] ")
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
    // file_view backs a single file by definition, so its key carries the
    // path. Status overlays use the workdir path; commit overlays use the
    // path inside the commit.
    let file_path: &str = match &app.diff.file_view.key {
        Some(crate::app::FileViewKey::Status(p)) => p.as_str(),
        Some(crate::app::FileViewKey::Commit { path, .. }) => path.as_str(),
        None => "",
    };
    let ext = super::path_extension(file_path);
    let syntax = ss
        .find_syntax_by_extension(ext)
        .unwrap_or_else(|| ss.find_syntax_plain_text());

    let has_search = app.diff.search.has_query();
    let show_search = app.diff.search.is_visible();

    let (content_area, search_area) = if show_search {
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Min(0), Constraint::Length(1)])
            .split(area);
        (chunks[0], Some(chunks[1]))
    } else {
        (area, None)
    };

    let title = if has_search {
        let count = app.diff.search.matches.len();
        if count == 0 {
            format!(" F2 {file_path} [no matches] ")
        } else {
            format!(
                " F2 {file_path} [{}/{}] ",
                app.diff.search.cursor + 1,
                count
            )
        }
    } else {
        format!(" F2 {file_path} [file] ")
    };

    let visible_height = (content_area.height as usize).saturating_sub(2);
    let current_match = app.diff.search.current_match();
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
                let line_idx = scroll_start + i;
                let is_anchor = fv.anchor_line == Some(line_no);
                let is_current = has_search && current_match == Some(line_idx);
                let is_match =
                    has_search && !is_current && app.diff.search.is_match(line_idx);
                let bg = if is_current {
                    Color::Rgb(100, 80, 0)
                } else if is_match {
                    Color::Rgb(50, 42, 0)
                } else if is_anchor {
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
    frame.render_widget(para, content_area);

    if let Some(sa) = search_area {
        super::render_search_bar(
            frame,
            app.diff.search.query.as_str(),
            app.diff.search.active,
            sa,
            accent,
        );
    }
}
