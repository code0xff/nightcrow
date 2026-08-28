use crate::app::{App, Focus};
use crate::ui::jump_legend;
use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Style},
    text::{Line, Span},
    widgets::{Block, Borders},
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
    let file_path: &str = match &app.diff_pane().file_view.key {
        Some(crate::app::FileViewKey::Status(p)) => p.as_str(),
        Some(crate::app::FileViewKey::Commit { path, .. }) => path.as_str(),
        None => "",
    };
    let ext = super::path_extension(file_path);
    let syntax = ss
        .find_syntax_by_extension(ext)
        .unwrap_or_else(|| ss.find_syntax_plain_text());

    let has_search = app.diff_pane().search.has_query();
    let show_search = app.diff_pane().search.is_visible();

    let (content_area, search_area) = if show_search {
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Min(0), Constraint::Length(1)])
            .split(area);
        (chunks[0], Some(chunks[1]))
    } else {
        (area, None)
    };

    let jump = jump_legend(app, '2');
    let title = if has_search {
        let count = app.diff_pane().search.matches.len();
        if count == 0 {
            format!(" {jump} {file_path} [no matches] ")
        } else {
            format!(
                " {jump} {file_path} [{}/{}] ",
                app.diff_pane().search.cursor + 1,
                count
            )
        }
    } else {
        format!(" {jump} {file_path} [file] ")
    };

    let visible_height = (content_area.height as usize).saturating_sub(2);
    let current_match = app.diff_pane().search.current_match();
    // An error or an empty file has no lines to number, so the gutter column is
    // not reserved at all — otherwise the message would sit indented under it.
    let mut gutter_lines: Vec<Line> = Vec::new();
    let mut gutter_width = 0u16;
    let lines: Vec<Line> = if let Some(err) = &app.diff_pane().file_view.error {
        vec![Line::from(Span::styled(
            err.as_str(),
            Style::default().fg(Color::Red),
        ))]
    } else if app.diff_pane().file_view.content.is_empty() {
        vec![Line::from(Span::styled(
            "(empty file)",
            Style::default().fg(Color::DarkGray),
        ))]
    } else {
        app.diff_pane_mut()
            .file_view
            .ensure_highlight_cache(ss, ts, syntax);
        let fv = &app.diff_pane().file_view;
        let total = fv.line_count();
        // Same floor as the diff gutters, so switching between `v` and the
        // diff view does not shift the body's left edge.
        let digits = super::gutter::digits_for(total);
        gutter_width = super::gutter::side_gutter_width(digits);
        // Belt-and-braces: if the highlight-cache invariant ever slips, the
        // slice below would panic — clamp against the cache length.
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
                    has_search && !is_current && app.diff_pane().search.is_match(line_idx);
                let bg = if is_current {
                    Color::Rgb(100, 80, 0)
                } else if is_match {
                    Color::Rgb(50, 42, 0)
                } else if is_anchor {
                    Color::Rgb(60, 60, 90)
                } else {
                    Color::Reset
                };
                // The number lives in its own paragraph so horizontal
                // scrolling cannot slide it off the left edge.
                gutter_lines.push(Line::from(Span::styled(
                    super::gutter::side_gutter_text(Some(line_no as u32), digits),
                    Style::default().fg(Color::DarkGray).bg(bg),
                )));
                let spans: Vec<Span> = segs
                    .iter()
                    .map(|seg| {
                        Span::styled(
                            seg.text.as_str(),
                            Style::default().fg(super::rgb_to_color(seg.rgb)).bg(bg),
                        )
                    })
                    .collect();
                Line::from(spans)
            })
            .collect()
    };

    let block = Block::default()
        .borders(Borders::ALL)
        .title(title)
        .border_style(border_style);
    let inner = block.inner(content_area);
    frame.render_widget(block, content_area);
    super::gutter::render_gutter_and_body(
        frame,
        inner,
        gutter_width,
        gutter_lines,
        lines,
        app.diff_pane().file_view.scroll_x.min(u16::MAX as usize) as u16,
        app.diff_pane().wrap,
    );

    if let Some(sa) = search_area {
        super::render_search_bar(
            frame,
            app.diff_pane().search.query.as_str(),
            app.diff_pane().search.active,
            sa,
            accent,
        );
    }
}
