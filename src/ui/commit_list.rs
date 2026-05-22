use crate::app::{App, Focus};
use ratatui::{
    Frame,
    layout::Rect,
    style::{Color, Style},
    text::{Line, Span},
    widgets::ListItem,
};
use std::time::{SystemTime, UNIX_EPOCH};

const SECS_PER_MINUTE: i64 = 60;
const SECS_PER_HOUR: i64 = 3_600;
const SECS_PER_DAY: i64 = 86_400;
const SECS_PER_MONTH: i64 = SECS_PER_DAY * 30;
const SECS_PER_YEAR: i64 = SECS_PER_DAY * 365;

fn format_relative_time(ts: i64) -> String {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);
    let secs = now.saturating_sub(ts).max(0);
    if secs < SECS_PER_MINUTE {
        format!("{secs}s")
    } else if secs < SECS_PER_HOUR {
        format!("{}m", secs / SECS_PER_MINUTE)
    } else if secs < SECS_PER_DAY {
        format!("{}h", secs / SECS_PER_HOUR)
    } else if secs < SECS_PER_MONTH {
        format!("{}d", secs / SECS_PER_DAY)
    } else if secs < SECS_PER_YEAR {
        format!("{}mo", secs / SECS_PER_MONTH)
    } else {
        format!("{}y", secs / SECS_PER_YEAR)
    }
}

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

    let ahead_count = app.tracking.as_ref().map_or(0, |t| t.ahead);

    let scroll_x = app.log_view.commit_scroll_x;
    let items: Vec<ListItem> = app
        .log_view
        .commits
        .iter()
        .enumerate()
        .map(|(i, entry)| {
            let time_str = format_relative_time(entry.time);
            let author_short: String = entry.author.chars().take(10).collect();
            let marker = if i < ahead_count { "↑ " } else { "  " };
            let summary = super::char_offset(&entry.summary, scroll_x);
            let line = Line::from(vec![
                Span::styled(marker, Style::default().fg(Color::Green)),
                Span::styled(format!("{} ", entry.short_id), Style::default().fg(accent)),
                Span::styled(
                    format!("{:>4} ", time_str),
                    Style::default().fg(Color::Gray),
                ),
                Span::styled(
                    format!("{:<10} ", author_short),
                    Style::default().fg(Color::Cyan),
                ),
                Span::raw(summary),
            ]);
            ListItem::new(line)
        })
        .collect();

    let title = if app.log_view.commits.is_empty() {
        " F1 Log (no commits) ".to_string()
    } else {
        format!(" F1 Log ({}) ", app.log_view.commits.len())
    };

    let selected = (!app.log_view.commits.is_empty()).then_some(app.log_view.selected);
    super::render_selectable_list(frame, area, title, items, selected, border_style);
}

fn render_file_list(frame: &mut Frame, app: &App, area: Rect, accent: Color) {
    let focused = app.focus == Focus::FileList;
    let border_style = super::focused_border_style(focused, accent);

    let scroll_x = app.log_view.file_scroll_x;
    let items: Vec<ListItem> = app
        .log_view
        .commit_files
        .iter()
        .map(|f| {
            let path = super::char_offset(&f.path, scroll_x);
            let line = Line::from(vec![
                Span::styled(
                    format!("{} ", f.status.symbol()),
                    Style::default().fg(super::status_color(f.status)),
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
        .map(|e| format!(" F1 {} {} ", e.short_id, e.summary))
        .unwrap_or_else(|| " F1 Files ".to_string());

    let title = truncate_title(&commit_summary, title_budget(area.width));

    let selected = (!app.log_view.commit_files.is_empty()).then_some(app.log_view.file_selected);
    super::render_selectable_list(frame, area, title, items, selected, border_style);
}

/// Char budget for the drill-down title inside `area`.
///
/// Reserves two cells for the surrounding border corners. The title is then
/// measured in chars (not display width), matching the trade-off documented
/// on `terminal_tab::truncate_tab_title`: ASCII summaries are the common
/// case and CJK titles render slightly under the visual budget.
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
    use super::{format_relative_time, truncate_title};

    #[test]
    fn format_relative_time_handles_far_future_timestamp() {
        // Corrupt/malicious commit timestamps must not panic on i64 underflow.
        let s = format_relative_time(i64::MAX);
        assert_eq!(s, "0s");
    }

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
