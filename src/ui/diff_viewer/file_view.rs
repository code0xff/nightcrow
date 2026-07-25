use crate::app::{App, Focus};
use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph},
};
use syntect::highlighting::ThemeSet;
use syntect::parsing::SyntaxSet;

pub(crate) fn render_file_view(
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

    let jump = super::jump_legend(app, '2');
    let title = if has_search {
        let count = app.diff.search.matches.len();
        if count == 0 {
            format!(" {jump} {file_path} [no matches] ")
        } else {
            format!(
                " {jump} {file_path} [{}/{}] ",
                app.diff.search.cursor + 1,
                count
            )
        }
    } else {
        format!(" {jump} {file_path} [file] ")
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
        // Belt-and-braces: ensure_highlight_cache keeps line_highlights
        // aligned with content.lines().count(), but if that invariant ever
        // slips the slice below would panic. Clamp against the cache length.
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
                let is_match = has_search && !is_current && app.diff.search.is_match(line_idx);
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
                        Style::default().fg(super::rgb_to_color(seg.rgb)).bg(bg),
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
