use crate::app::{App, Focus};
use crate::ui::hot_stage::{HotStage, classify_hot};
use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::ListItem,
};
use std::time::{Duration, SystemTime};

/// Find the earliest stage transition visible in the active status list.
/// `StatusView` memoizes the scan until its inputs change or the deadline
/// passes, so the event loop's tick costs O(1) in the steady state.
pub(crate) fn next_hot_deadline_for_app(app: &App, now: SystemTime) -> Option<SystemTime> {
    if app.mode() != crate::app::ViewMode::Status || !app.agent_indicator_config().enabled {
        return None;
    }
    let hot_window = Duration::from_secs(app.agent_indicator_config().hot_window_secs);
    app.status_view().next_hot_deadline(now, hot_window)
}

pub fn render(frame: &mut Frame, app: &App, area: Rect, accent: Color) {
    render_with(frame, app, area, accent, super::RowsScope::Window);
}

#[cfg(test)]
pub(super) fn render_full(frame: &mut Frame, app: &App, area: Rect, accent: Color) {
    render_with(frame, app, area, accent, super::RowsScope::Full);
}

fn render_with(frame: &mut Frame, app: &App, area: Rect, accent: Color, scope: super::RowsScope) {
    let focused = app.focus == Focus::FileList;
    let border_style = super::focused_border_style(focused, accent);

    let show_search = app.status_view().search_active || !app.status_view().search_query.is_empty();

    let (list_area, search_area) = if show_search {
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Min(0), Constraint::Length(1)])
            .split(area);
        (chunks[0], Some(chunks[1]))
    } else {
        (area, None)
    };

    let filtered_indices = app.filtered_indices();
    let match_count = filtered_indices.len();

    let indicator_enabled = app.agent_indicator_config().enabled;
    let hot_window = Duration::from_secs(app.agent_indicator_config().hot_window_secs);
    let now = SystemTime::now();

    let selected_pos = super::filtered_position(filtered_indices, app.status_view().selected);
    let window = super::rows_to_render(scope, filtered_indices.len(), list_area, selected_pos);
    let items = file_items(
        app,
        filtered_indices,
        window.clone(),
        indicator_enabled,
        hot_window,
        now,
        accent,
    );

    let title = if show_search {
        format!(
            " {} Files ({}/{}) ",
            super::jump_legend(app, '1'),
            match_count,
            app.status_view().files.len()
        )
    } else if app.status_view().files.is_empty() {
        format!(" {} Files (no changes) ", super::jump_legend(app, '1'))
    } else {
        format!(" {} Files ", super::jump_legend(app, '1'))
    };

    let selected_pos = super::windowed_selection(selected_pos, &window);
    super::render_selectable_list(frame, list_area, title, items, selected_pos, border_style);

    if let Some(sa) = search_area {
        super::render_search_bar(
            frame,
            app.status_view().search_query.as_str(),
            app.status_view().search_active,
            sa,
            accent,
        );
    }
}

fn file_items<'a>(
    app: &'a App,
    filtered_indices: &[usize],
    window: std::ops::Range<usize>,
    indicator_enabled: bool,
    hot_window: Duration,
    now: SystemTime,
    accent: Color,
) -> Vec<ListItem<'a>> {
    let status = app.status_view();
    let scroll_x = status.file_scroll_x;
    filtered_indices[window]
        .iter()
        .map(|&idx| {
            let f = &status.files[idx];
            let symbol = f.short_code();
            let color = super::status_color(f.most_severe());
            // Borrow `f.path` in the common non-rename case so rendering stays
            // allocation-free; only renames, whose `old -> new` display string
            // is owned, allocate.
            let path: std::borrow::Cow<'_, str> = match f.display_path() {
                std::borrow::Cow::Borrowed(_) => {
                    std::borrow::Cow::Borrowed(super::char_offset(&f.path, scroll_x))
                }
                std::borrow::Cow::Owned(display) => {
                    std::borrow::Cow::Owned(super::char_offset(&display, scroll_x).to_string())
                }
            };

            let stage = if indicator_enabled {
                status
                    .hot_table
                    .get(&f.path)
                    .map(|m| classify_hot(*m, now, hot_window))
                    .unwrap_or(HotStage::Cool)
            } else {
                HotStage::Cool
            };

            // The status symbol keeps its change-status color across all hot
            // stages so the change kind stays readable; recency is conveyed by
            // path styling only, so stage transitions don't shift the row.
            let line = match stage {
                HotStage::Cool => Line::from(vec![
                    Span::styled(format!("{symbol} "), Style::default().fg(color)),
                    Span::raw(path),
                ]),
                HotStage::Fresh => Line::from(vec![
                    Span::styled(format!("{symbol} "), Style::default().fg(color)),
                    Span::styled(
                        path,
                        Style::default().fg(accent).add_modifier(Modifier::BOLD),
                    ),
                ]),
                HotStage::Warm => Line::from(vec![
                    Span::styled(format!("{symbol} "), Style::default().fg(color)),
                    Span::styled(path, Style::default().fg(accent)),
                ]),
            };
            ListItem::new(line)
        })
        .collect()
}
