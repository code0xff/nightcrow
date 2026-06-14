use crate::app::{App, Focus};
use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::ListItem,
};
use std::time::{Duration, SystemTime};

/// Stages of the agent-aware focus indicator fade. `Cool` means the file is
/// outside the configured hot window and renders identically to the legacy
/// (pre-feature) row.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum HotStage {
    Fresh,
    Warm,
    Cool,
}

/// Bucket a single mtime against `now` and the user's hot window. The
/// "fresh" threshold sits well above typical filesystem mtime granularity
/// (1s on FAT/older ext4) so the bold→non-bold transition remains easy to
/// spot. Negative deltas (clock skew, mtime in the future) saturate to
/// `Fresh` — the conservative "just touched" choice.
fn classify_hot(mtime: SystemTime, now: SystemTime, hot_window: Duration) -> HotStage {
    let age = now.duration_since(mtime).unwrap_or(Duration::ZERO);
    if age >= hot_window {
        HotStage::Cool
    } else if age < Duration::from_secs(5) {
        HotStage::Fresh
    } else {
        HotStage::Warm
    }
}

pub fn render(frame: &mut Frame, app: &App, area: Rect, accent: Color) {
    let focused = app.focus == Focus::FileList;
    let border_style = super::focused_border_style(focused, accent);

    let show_search = app.status_view.search_active || !app.status_view.search_query.is_empty();

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

    let indicator_enabled = app.cfg_agent_indicator.enabled;
    let hot_window = Duration::from_secs(app.cfg_agent_indicator.hot_window_secs);
    let now = SystemTime::now();

    let items: Vec<ListItem> = filtered_indices
        .iter()
        .map(|&idx| {
            let f = &app.status_view.files[idx];
            let symbol = f.short_code();
            let color = super::status_color(f.most_severe());
            let scroll_x = app.status_view.file_scroll_x;
            // Borrow `f.path` (which outlives the item list) in the common
            // non-rename case so rendering stays allocation-free; only renames,
            // whose `old -> new` display string is owned, allocate.
            let path: std::borrow::Cow<'_, str> = match f.display_path() {
                std::borrow::Cow::Borrowed(_) => {
                    std::borrow::Cow::Borrowed(super::char_offset(&f.path, scroll_x))
                }
                std::borrow::Cow::Owned(display) => {
                    std::borrow::Cow::Owned(super::char_offset(&display, scroll_x).to_string())
                }
            };

            let stage = if indicator_enabled {
                app.status_view
                    .hot_table
                    .get(&f.path)
                    .map(|m| classify_hot(*m, now, hot_window))
                    .unwrap_or(HotStage::Cool)
            } else {
                HotStage::Cool
            };

            // The status symbol keeps its change-status color across all hot
            // stages so the change kind (added/modified/…) stays readable.
            // Recency is conveyed by path styling only — no leading glyph —
            // so transitions between stages don't shift the row width.
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
        .collect();

    let title = if show_search {
        format!(
            " F1 Files ({}/{}) ",
            match_count,
            app.status_view.files.len()
        )
    } else if app.status_view.files.is_empty() {
        " F1 Files (no changes) ".to_string()
    } else {
        " F1 Files ".to_string()
    };

    let selected_pos = filtered_indices
        .iter()
        .position(|&i| i == app.status_view.selected);
    super::render_selectable_list(frame, list_area, title, items, selected_pos, border_style);

    if let Some(sa) = search_area {
        super::render_search_bar(
            frame,
            app.status_view.search_query.as_str(),
            app.status_view.search_active,
            sa,
            accent,
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classify_hot_buckets_fresh_warm_cool() {
        let window = Duration::from_secs(10);
        let now = SystemTime::UNIX_EPOCH + Duration::from_secs(100);

        let fresh = SystemTime::UNIX_EPOCH + Duration::from_secs(99);
        assert_eq!(classify_hot(fresh, now, window), HotStage::Fresh);

        let warm = SystemTime::UNIX_EPOCH + Duration::from_secs(95);
        assert_eq!(classify_hot(warm, now, window), HotStage::Warm);

        let cool = SystemTime::UNIX_EPOCH + Duration::from_secs(80);
        assert_eq!(classify_hot(cool, now, window), HotStage::Cool);
    }

    #[test]
    fn classify_hot_clamps_future_mtime_to_fresh() {
        let window = Duration::from_secs(10);
        let now = SystemTime::UNIX_EPOCH + Duration::from_secs(100);
        let future = SystemTime::UNIX_EPOCH + Duration::from_secs(110);
        assert_eq!(classify_hot(future, now, window), HotStage::Fresh);
    }

    #[test]
    fn classify_hot_window_boundary_is_cool() {
        let window = Duration::from_secs(10);
        let now = SystemTime::UNIX_EPOCH + Duration::from_secs(100);
        let on_boundary = SystemTime::UNIX_EPOCH + Duration::from_secs(90);
        assert_eq!(classify_hot(on_boundary, now, window), HotStage::Cool);
    }
}
