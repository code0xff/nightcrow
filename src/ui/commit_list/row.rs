use crate::git::diff::{CommitEntry, LogDecorations, RefKind, RefLabel};
use crate::ui::wall_clock::local_date_time;
use ratatui::{
    style::{Color, Modifier, Style},
    text::{Line, Span},
};
use std::time::{SystemTime, UNIX_EPOCH};

const SECS_PER_MINUTE: i64 = 60;
const SECS_PER_HOUR: i64 = 3_600;
const SECS_PER_DAY: i64 = 86_400;
const SECS_PER_MONTH: i64 = SECS_PER_DAY * 30;
const SECS_PER_YEAR: i64 = SECS_PER_DAY * 365;

/// Terminal width at which the row switches to absolute time, full author, and
/// untruncated ref chips. A width rule rather than the `list_fullscreen` flag:
/// a wide monitor has the room outside fullscreen too, and it keeps the
/// decision to one threshold — the same shape as `diff_viewer::MIN_SPLIT_WIDTH`.
pub(super) const MIN_DETAIL_WIDTH: u16 = 120;

const AUTHOR_WIDTH: usize = 10;
const WIDE_AUTHOR_WIDTH: usize = 24;
const WIDE_SHORT_ID: usize = 10;
/// Chip budget in the narrow layout, as a fraction of the row. Chips compete
/// with the summary there, so they get a slice rather than the whole width.
const NARROW_CHIP_BUDGET: usize = 24;

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

/// Divergence glyph (`↑` ahead of upstream, `v` behind) and shape glyph
/// (`*` HEAD, `Y` merge). Two fixed cells so rows stay column-aligned.
fn glyphs(entry: &CommitEntry, decorations: &LogDecorations) -> (Span<'static>, Span<'static>) {
    let divergence = if decorations.is_ahead(entry.oid) {
        Span::styled("↑", Style::default().fg(Color::Green))
    } else if decorations.is_behind(entry.oid) {
        Span::styled("v", Style::default().fg(Color::Yellow))
    } else {
        Span::raw(" ")
    };
    let shape = if decorations.is_head(entry.oid) {
        Span::styled(
            "*",
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        )
    } else if entry.is_merge() {
        Span::styled("Y", Style::default().fg(Color::Magenta))
    } else {
        Span::raw(" ")
    };
    (divergence, shape)
}

fn chip_style(kind: RefKind) -> Style {
    // git's own `--decorate` palette, so the colors read the same as the CLI.
    match kind {
        RefKind::Head => Style::default()
            .fg(Color::Cyan)
            .add_modifier(Modifier::BOLD),
        RefKind::LocalBranch => Style::default().fg(Color::Green),
        RefKind::Tag => Style::default().fg(Color::Yellow),
        RefKind::RemoteBranch => Style::default().fg(Color::Red),
    }
}

fn chip_text(label: &RefLabel) -> String {
    match label.kind {
        RefKind::Head if label.name != "HEAD" => format!("HEAD -> {}", label.name),
        _ => label.name.clone(),
    }
}

/// Chips for one commit, dropped from the end once `budget` chars are used.
/// Labels arrive sorted by [`RefKind`] priority (HEAD > local > tag > remote),
/// so a short budget keeps the most orienting ones.
fn chip_spans(labels: &[RefLabel], budget: usize) -> Vec<Span<'static>> {
    let mut spans = Vec::new();
    let mut used = 0;
    for label in labels {
        let text = chip_text(label);
        let width = text.chars().count() + 1;
        if used + width > budget {
            break;
        }
        used += width;
        spans.push(Span::styled(text, chip_style(label.kind)));
        spans.push(Span::raw(" "));
    }
    spans
}

pub(super) fn commit_row<'a>(
    entry: &'a CommitEntry,
    decorations: &LogDecorations,
    width: u16,
    scroll_x: usize,
    accent: Color,
) -> Line<'a> {
    let wide = width >= MIN_DETAIL_WIDTH;
    let (divergence, shape) = glyphs(entry, decorations);
    let mut spans = vec![divergence, shape, Span::raw(" ")];

    let id = if wide {
        entry.oid.to_string().chars().take(WIDE_SHORT_ID).collect()
    } else {
        entry.short_id.clone()
    };
    spans.push(Span::styled(format!("{id} "), Style::default().fg(accent)));

    if wide {
        // A timestamp the platform cannot place renders as blanks rather than a
        // wrong date, keeping the column aligned. Same contract as `wall_clock`.
        let stamp = local_date_time(entry.time).unwrap_or_else(|| " ".repeat(16));
        spans.push(Span::styled(
            format!("{stamp} "),
            Style::default().fg(Color::Gray),
        ));
        let who = match entry.author_email.as_str() {
            "" => entry.author.clone(),
            email => format!("{} <{email}>", entry.author),
        };
        let who: String = who.chars().take(WIDE_AUTHOR_WIDTH).collect();
        spans.push(Span::styled(
            format!("{who:<WIDE_AUTHOR_WIDTH$} "),
            Style::default().fg(Color::Cyan),
        ));
    } else {
        spans.push(Span::styled(
            format!("{:>4} ", format_relative_time(entry.time)),
            Style::default().fg(Color::Gray),
        ));
        let author: String = entry.author.chars().take(AUTHOR_WIDTH).collect();
        spans.push(Span::styled(
            format!("{author:<AUTHOR_WIDTH$} "),
            Style::default().fg(Color::Cyan),
        ));
    }

    let budget = if wide { usize::MAX } else { NARROW_CHIP_BUDGET };
    spans.extend(chip_spans(decorations.labels_for(entry.oid), budget));

    spans.push(Span::raw(crate::ui::char_offset(&entry.summary, scroll_x)));
    Line::from(spans)
}

#[cfg(test)]
mod tests {
    use super::{RefKind, RefLabel, chip_spans, chip_text, format_relative_time};

    fn label(kind: RefKind, name: &str) -> RefLabel {
        RefLabel {
            kind,
            name: name.to_string(),
        }
    }

    #[test]
    fn format_relative_time_handles_far_future_timestamp() {
        // Corrupt/malicious commit timestamps must not panic on i64 underflow.
        let s = format_relative_time(i64::MAX);
        assert_eq!(s, "0s");
    }

    #[test]
    fn the_head_chip_names_the_branch_it_points_at() {
        assert_eq!(chip_text(&label(RefKind::Head, "dev")), "HEAD -> dev");
    }

    #[test]
    fn a_detached_head_chip_stays_bare() {
        assert_eq!(chip_text(&label(RefKind::Head, "HEAD")), "HEAD");
    }

    #[test]
    fn a_tight_budget_keeps_the_higher_priority_chips() {
        let labels = vec![
            label(RefKind::Head, "dev"),
            label(RefKind::RemoteBranch, "origin/dev"),
        ];

        let spans = chip_spans(&labels, 12);

        // "HEAD -> dev" plus its trailing space exactly fills the budget.
        let text: String = spans.iter().map(|s| s.content.as_ref()).collect();
        assert_eq!(text, "HEAD -> dev ");
    }

    #[test]
    fn a_budget_too_small_for_any_chip_renders_none() {
        let labels = vec![label(RefKind::RemoteBranch, "origin/dev")];

        assert!(chip_spans(&labels, 4).is_empty());
    }
}
