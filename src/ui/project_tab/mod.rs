//! The project tab strip: a row across the top of the screen, or a column
//! down its left (`[layout] tabs`). One `tab_segments` builder feeds both the
//! renderer and the click hit-test, so a label and its click box can never
//! disagree — and both placements share it, so a project is called the same
//! thing and overflows the same way whichever way the strip runs.

mod window;

use crate::config::TabStrip;
use ratatui::{
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::Paragraph,
};
use std::time::Duration;
use window::tab_segments;

/// Per-tab character budget for the project name. The viewer's tab row applies
/// the same budget by the same rule (`viewer-ui/src/lib/tabLabel.ts`), so a
/// project is called the same thing on both screens.
const TAB_TITLE_MAX_CHARS: usize = 14;

/// The column a left-placed strip takes (`[layout] tabs = "left"`): the widest
/// legend (` F10•`, five cells), the label budget above, and one cell of
/// padding. Fixed rather than configurable because it is derived from the
/// label rule — widening it would show nothing more, and narrowing it would
/// cut labels the rule had already cut.
pub(crate) const STRIP_WIDTH: u16 = 20;

const ATTENTION_GLYPH: char = '•';
const ATTENTION_BLINK_INTERVAL: Duration = Duration::from_secs(1);

/// Only style changes between phases, so the strip and its pointer hit boxes
/// never move while it blinks.
pub(crate) fn blink_is_bright(elapsed: Duration) -> bool {
    (elapsed.as_millis() / ATTENTION_BLINK_INTERVAL.as_millis()).is_multiple_of(2)
}

/// Goes through `Path` rather than splitting on `/` so a Windows path
/// (`C:\work\api`) yields `api` too.
pub(crate) fn tab_label(repo_path: &str) -> String {
    let path = std::path::Path::new(repo_path);
    let name = path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| repo_path.to_string());
    if name.is_empty() {
        return "?".to_string();
    }
    truncate(&name, TAB_TITLE_MAX_CHARS)
}

/// Truncate to at most `max` characters, appending `…` when cut.
fn truncate(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        return s.to_string();
    }
    let mut out: String = s.chars().take(max.saturating_sub(1)).collect();
    out.push('…');
    out
}

/// What a segment's extent is measured in: cells across a row, rows down a
/// column.
fn budget_of(area: Rect, strip: TabStrip) -> u16 {
    match strip {
        TabStrip::Top => area.width,
        TabStrip::Left => area.height,
    }
}

/// Draw the strip into `area`. A single project still renders its tab: the
/// strip is permanent (see `chrome_areas`), and showing which repo is open is
/// exactly what it is for.
///
/// A column pads every segment to the strip's width, so the active tab's
/// accent fills its whole row and the row is the click box — a label that
/// stopped short would leave dead cells beside it that look like the tab.
pub(crate) fn render(
    repo_paths: &[String],
    attention: &[bool],
    active: usize,
    area: Rect,
    accent: Color,
    attention_bright: bool,
    strip: TabStrip,
) -> Paragraph<'static> {
    let segments = tab_segments(repo_paths, attention, active, budget_of(area, strip), strip);
    let style = |text: String, index: usize| {
        styled_spans(text, index, active, attention, accent, attention_bright)
    };
    match strip {
        TabStrip::Top => Paragraph::new(Line::from(
            segments
                .into_iter()
                .flat_map(|(text, index)| style(text, index))
                .collect::<Vec<Span>>(),
        )),
        TabStrip::Left => Paragraph::new(
            segments
                .into_iter()
                .map(|(text, index)| Line::from(style(pad_to(text, area.width), index)))
                .collect::<Vec<Line>>(),
        ),
    }
}

/// Fill `text` out to `width` cells, or cut it there: a segment is one row of
/// the column and must be exactly that wide. Cut by cells, not characters — a
/// CJK name is two cells a character, and a wide glyph that would straddle the
/// edge is dropped rather than split.
fn pad_to(text: String, width: u16) -> String {
    let width = width as usize;
    let mut shown = String::new();
    let mut used = 0;
    for ch in text.chars() {
        let cells = Span::raw(ch.to_string()).width();
        if used + cells > width {
            break;
        }
        shown.push(ch);
        used += cells;
    }
    format!("{shown}{}", " ".repeat(width - used))
}

/// One segment's spans. A `+N` marker is never the active tab, so accent stays
/// a reliable "this is the project you are in" signal.
fn styled_spans(
    text: String,
    index: usize,
    active: usize,
    attention: &[bool],
    accent: Color,
    attention_bright: bool,
) -> Vec<Span<'static>> {
    let is_marker = text.trim_start().starts_with('+');
    if index == active && !is_marker {
        return vec![Span::styled(
            text,
            Style::default()
                .fg(Color::Black)
                .bg(accent)
                .add_modifier(Modifier::BOLD),
        )];
    }
    let base = if is_marker {
        Style::default().fg(Color::DarkGray)
    } else {
        Style::default().fg(Color::Gray)
    };
    let has_attention = if is_marker {
        text.trim_end().ends_with(ATTENTION_GLYPH)
    } else {
        attention.get(index).copied().unwrap_or(false)
    };
    // The glyph is not always there to style: a column on a terminal narrower
    // than the strip has cut its rows to fit (`pad_to`), and the marker sits
    // far enough along ` F10•` to be the first thing cut. A tab with attention
    // and no room to show it is drawn plain rather than not drawn at all.
    let Some(marker_start) = text.find(ATTENTION_GLYPH).filter(|_| has_attention) else {
        return vec![Span::styled(text, base)];
    };
    let marker_end = marker_start + ATTENTION_GLYPH.len_utf8();
    let marker_style = Style::default()
        .fg(if attention_bright {
            accent
        } else {
            Color::DarkGray
        })
        .add_modifier(Modifier::BOLD);
    vec![
        Span::styled(text[..marker_start].to_string(), base),
        Span::styled(ATTENTION_GLYPH.to_string(), marker_style),
        Span::styled(text[marker_end..].to_string(), base),
    ]
}

/// The project index a click at screen cell `(x, y)` selects, or `None` off
/// the strip or past the last tab.
pub(crate) fn tab_at(
    repo_paths: &[String],
    attention: &[bool],
    active: usize,
    area: Rect,
    x: u16,
    y: u16,
    strip: TabStrip,
) -> Option<usize> {
    // On a terminal too small for the full chrome, ratatui hands the fixed
    // constraint a zero-size Rect and nothing is drawn; without the size check
    // a click on whatever is visible there would select tab 0.
    if area.height == 0 || area.width == 0 || x < area.x || y < area.y {
        return None;
    }
    let segments = tab_segments(repo_paths, attention, active, budget_of(area, strip), strip);
    match strip {
        TabStrip::Top => {
            if y != area.y {
                return None;
            }
            let mut cursor = area.x;
            for (text, index) in segments {
                let width = Span::raw(text).width() as u16;
                if x < cursor.saturating_add(width) {
                    return Some(index);
                }
                cursor = cursor.saturating_add(width);
            }
            None
        }
        TabStrip::Left => {
            // Both edges, not just the left and the top: on a terminal too short
            // for the active tab and its two markers the segments outnumber the
            // rows, and a click on the notice row under the strip would otherwise
            // index the segment that was never drawn.
            if x >= area.x.saturating_add(area.width) || y >= area.y.saturating_add(area.height) {
                return None;
            }
            segments
                .get(usize::from(y - area.y))
                .map(|(_, index)| *index)
        }
    }
}

#[cfg(test)]
mod tests;
