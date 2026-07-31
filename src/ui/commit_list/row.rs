use crate::git::diff::CommitEntry;
use ratatui::{
    style::{Color, Style},
    text::{Line, Span},
};
use std::time::{SystemTime, UNIX_EPOCH};

const SECS_PER_MINUTE: i64 = 60;
const SECS_PER_HOUR: i64 = 3_600;
const SECS_PER_DAY: i64 = 86_400;
const SECS_PER_MONTH: i64 = SECS_PER_DAY * 30;
const SECS_PER_YEAR: i64 = SECS_PER_DAY * 365;

const AUTHOR_WIDTH: usize = 10;

pub(super) fn format_relative_time(ts: i64) -> String {
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

pub(super) fn commit_row<'a>(
    entry: &'a CommitEntry,
    ahead: bool,
    scroll_x: usize,
    accent: Color,
) -> Line<'a> {
    let time_str = format_relative_time(entry.time);
    let author_short: String = entry.author.chars().take(AUTHOR_WIDTH).collect();
    let marker = if ahead { "↑ " } else { "  " };
    let summary = crate::ui::char_offset(&entry.summary, scroll_x);
    Line::from(vec![
        Span::styled(marker, Style::default().fg(Color::Green)),
        Span::styled(format!("{} ", entry.short_id), Style::default().fg(accent)),
        Span::styled(
            format!("{:>4} ", time_str),
            Style::default().fg(Color::Gray),
        ),
        Span::styled(
            format!("{author_short:<AUTHOR_WIDTH$} "),
            Style::default().fg(Color::Cyan),
        ),
        Span::raw(summary),
    ])
}

#[cfg(test)]
mod tests {
    use super::format_relative_time;

    #[test]
    fn format_relative_time_handles_far_future_timestamp() {
        // Corrupt/malicious commit timestamps must not panic on i64 underflow.
        let s = format_relative_time(i64::MAX);
        assert_eq!(s, "0s");
    }
}
