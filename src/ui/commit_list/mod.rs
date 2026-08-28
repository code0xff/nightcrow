mod row;

use crate::app::{App, Focus};
use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Style},
    text::{Line, Span},
    widgets::ListItem,
};

pub fn render(frame: &mut Frame, app: &App, area: Rect, accent: Color) {
    if app.log_view.drill_down {
        render_file_list(frame, app, area, accent);
    } else {
        render_commit_list(frame, app, area, accent);
    }
}

fn render_commit_list(frame: &mut Frame, app: &App, area: Rect, accent: Color) {
    let focused = app.focus == Focus::FileList;
    let border_style = super::focused_border_style(focused, accent);

    let show_search =
        app.log_view.commit_search_active || !app.log_view.commit_search_query.is_empty();

    let (list_area, search_area) = if show_search {
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Min(0), Constraint::Length(1)])
            .split(area);
        (chunks[0], Some(chunks[1]))
    } else {
        (area, None)
    };

    let filtered = app.log_commit_filtered_indices();
    let match_count = filtered.len();
    let total_count = app.log_view.commits.len();

    let scroll_x = app.log_view.commit_scroll_x;
    let items: Vec<ListItem> = filtered
        .iter()
        .map(|&i| {
            let entry = &app.log_view.commits[i];
            ListItem::new(row::commit_row(
                entry,
                &app.log_decorations,
                list_area.width,
                scroll_x,
                accent,
            ))
        })
        .collect();

    let title = if total_count == 0 {
        format!(" {} Log (no commits) ", super::jump_legend(app, '1'))
    } else if show_search {
        format!(
            " {} Log ({match_count}/{total_count}) ",
            super::jump_legend(app, '1')
        )
    } else {
        format!(" {} Log ({total_count}) ", super::jump_legend(app, '1'))
    };

    let selected_pos = filtered.iter().position(|&i| i == app.log_view.selected);
    super::render_selectable_list(frame, list_area, title, items, selected_pos, border_style);

    if let Some(sa) = search_area {
        super::render_search_bar(
            frame,
            app.log_view.commit_search_query.as_str(),
            app.log_view.commit_search_active,
            sa,
            accent,
        );
    }
}

fn render_file_list(frame: &mut Frame, app: &App, area: Rect, accent: Color) {
    let focused = app.focus == Focus::FileList;
    let border_style = super::focused_border_style(focused, accent);

    let show_search = app.log_view.file_search_active || !app.log_view.file_search_query.is_empty();

    let (list_area, search_area) = if show_search {
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Min(0), Constraint::Length(1)])
            .split(area);
        (chunks[0], Some(chunks[1]))
    } else {
        (area, None)
    };

    let filtered = app.log_file_filtered_indices();
    let match_count = filtered.len();
    let total_count = app.log_view.commit_files.len();

    let scroll_x = app.log_view.file_scroll_x;
    let items: Vec<ListItem> = filtered
        .iter()
        .map(|&i| {
            let f = &app.log_view.commit_files[i];
            let path: std::borrow::Cow<'_, str> = match f.display_path() {
                std::borrow::Cow::Borrowed(_) => {
                    std::borrow::Cow::Borrowed(super::char_offset(&f.path, scroll_x))
                }
                std::borrow::Cow::Owned(display) => {
                    std::borrow::Cow::Owned(super::char_offset(&display, scroll_x).to_string())
                }
            };
            let line = Line::from(vec![
                Span::styled(
                    format!("{} ", f.short_code()),
                    Style::default().fg(super::status_color(f.most_severe())),
                ),
                Span::raw(path),
            ]);
            ListItem::new(line)
        })
        .collect();

    let commit_summary = app
        .log_view
        .commits
        .get(app.log_view.selected)
        .map(|e| {
            format!(
                " {} {} {} ",
                super::jump_legend(app, '1'),
                e.short_id,
                e.summary
            )
        })
        .unwrap_or_else(|| format!(" {} Files ", super::jump_legend(app, '1')));

    let title_base = truncate_title(&commit_summary, title_budget(list_area.width));
    let title = if show_search && total_count > 0 {
        // Drop the trailing space the base title carries so the count suffix
        // sits flush against the summary text.
        format!("{} ({match_count}/{total_count}) ", title_base.trim_end())
    } else {
        title_base
    };

    let selected_pos = filtered
        .iter()
        .position(|&i| i == app.log_view.file_selected);
    super::render_selectable_list(frame, list_area, title, items, selected_pos, border_style);

    if let Some(sa) = search_area {
        super::render_search_bar(
            frame,
            app.log_view.file_search_query.as_str(),
            app.log_view.file_search_active,
            sa,
            accent,
        );
    }
}

/// Char budget for the drill-down title inside `area`, reserving two cells
/// for the border corners. Measured in chars (not display width), matching
/// the trade-off documented on `terminal_tab::truncate_tab_title`.
fn title_budget(width: u16) -> usize {
    (width as usize).saturating_sub(2)
}

fn truncate_title(title: &str, max_chars: usize) -> String {
    if title.chars().count() > max_chars {
        format!(
            "{}...",
            title
                .chars()
                .take(max_chars.saturating_sub(3))
                .collect::<String>()
        )
    } else {
        title.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::truncate_title;

    #[test]
    fn truncate_title_handles_multibyte_text() {
        let title = " abc1234 한글 커밋 메시지 제목이 꽤 길어서 잘려야 합니다 ";

        let truncated = truncate_title(title, 30);

        assert!(truncated.ends_with("..."));
        assert!(truncated.chars().count() <= 30);
    }

    #[test]
    fn truncate_title_keeps_short_text() {
        let title = " abc1234 short ";

        assert_eq!(truncate_title(title, 30), title);
    }
}
