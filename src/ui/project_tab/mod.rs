//! The project tab row across the top of the screen. One `tab_segments`
//! builder feeds both the renderer and the click hit-test, so a label and its
//! click box can never disagree.

use ratatui::{
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::Paragraph,
};
use std::time::Duration;

/// Per-tab character budget for the project name. The viewer's tab row applies
/// the same budget by the same rule (`viewer-ui/src/lib/tabLabel.ts`), so a
/// project is called the same thing on both screens — widening one without the
/// other is how they come to disagree.
const TAB_TITLE_MAX_CHARS: usize = 14;

/// Width of a `+N` overflow marker.
const MARKER_WIDTH: u16 = 4;

const ATTENTION_GLYPH: char = '•';
const ATTENTION_BLINK_INTERVAL: Duration = Duration::from_secs(1);

/// Bright/dim phase for the unread marker. Only style changes between phases,
/// so the row and its pointer hit boxes never move while it blinks.
pub(crate) fn blink_is_bright(elapsed: Duration) -> bool {
    (elapsed.as_millis() / ATTENTION_BLINK_INTERVAL.as_millis()).is_multiple_of(2)
}

/// The name shown for a repo path — its final component. Goes through `Path`
/// rather than splitting on `/` so a Windows path (`C:\work\api`) yields
/// `api` too.
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

/// The full text of every tab, ignoring how many will fit. Every tab carries
/// its `F#` legend because the F-key row addresses projects directly and
/// layout-independently. Projects past the tenth have no key, so they carry
/// no legend rather than implying an unbound one.
fn tab_texts(repo_paths: &[String], attention: &[bool]) -> Vec<String> {
    repo_paths
        .iter()
        .enumerate()
        .map(|(i, path)| {
            let name = tab_label(path);
            let unread = attention.get(i).copied().unwrap_or(false);
            match (i.checked_add(1).filter(|n| *n <= 10), unread) {
                (Some(n), true) => format!(" F{n}{ATTENTION_GLYPH}{name} "),
                (Some(n), false) => format!(" F{n} {name} "),
                (None, true) => format!(" {ATTENTION_GLYPH}{name}"),
                (None, false) => format!(" {name} "),
            }
        })
        .collect()
}

/// The run of tabs to draw in `width` cells, always containing `active`.
/// Ten tabs of repo names do not fit an 80-column row, and a `Paragraph`
/// would clip the tail — silently hiding later projects *and* the active-tab
/// highlight when the active one falls off the end. So the row scrolls
/// around the active tab, and what is dropped is replaced by a `+N` marker
/// whose width is reserved here before deciding what fits.
fn visible_window(widths: &[u16], width: u16, active: usize) -> std::ops::Range<usize> {
    let n = widths.len();
    if n == 0 {
        return 0..0;
    }
    let active = active.min(n - 1);
    let (mut lo, mut hi) = (active, active + 1);
    let mut used = widths[active];

    // Cost of the window if it were [lo, hi): its tabs plus a marker on each
    // side that still has something hidden behind it.
    let fits = |used: u16, lo: usize, hi: usize| {
        let markers = (lo > 0) as u16 + (hi < n) as u16;
        used.saturating_add(markers * MARKER_WIDTH) <= width
    };

    // Grow right first, then left. Right-first keeps the common case (active
    // near the front) showing the projects that follow it.
    loop {
        let mut grew = false;
        if hi < n && fits(used + widths[hi], lo, hi + 1) {
            used += widths[hi];
            hi += 1;
            grew = true;
        }
        if lo > 0 && fits(used + widths[lo - 1], lo - 1, hi) {
            lo -= 1;
            used += widths[lo];
            grew = true;
        }
        if !grew {
            return lo..hi;
        }
    }
}

/// Build the row's segments: rendered text paired with the project each one
/// selects. Single source for `render` and `tab_at`, so the hit boxes always
/// match what is on screen. A `+N` marker selects the nearest project hidden
/// on its side, so the overflow is reachable by pointer as well as by F-key.
fn tab_segments(
    repo_paths: &[String],
    attention: &[bool],
    active: usize,
    width: u16,
) -> Vec<(String, usize)> {
    let texts = tab_texts(repo_paths, attention);
    let widths: Vec<u16> = texts.iter().map(|t| Span::raw(t).width() as u16).collect();
    let visible = visible_window(&widths, width, active);

    let mut segments = Vec::with_capacity(visible.len() + 2);
    if visible.start > 0 {
        let marker = if attention[..visible.start.min(attention.len())]
            .iter()
            .any(|unread| *unread)
        {
            format!(" +{}{ATTENTION_GLYPH}", visible.start)
        } else {
            format!(" +{} ", visible.start)
        };
        segments.push((marker, visible.start - 1));
    }
    segments.extend(
        texts[visible.clone()]
            .iter()
            .enumerate()
            .map(|(offset, text)| (text.clone(), visible.start + offset)),
    );
    let hidden_after = texts.len() - visible.end;
    if hidden_after > 0 {
        let marker = if attention
            .get(visible.end..)
            .is_some_and(|hidden| hidden.iter().any(|unread| *unread))
        {
            format!(" +{hidden_after}{ATTENTION_GLYPH}")
        } else {
            format!(" +{hidden_after} ")
        };
        segments.push((marker, visible.end));
    }
    segments
}

/// Draw the tab row into `area`. A single project still renders its tab: the
/// row is permanent (see `chrome_rows`), and showing which repo is open is
/// exactly what the row is for. `accent` marks the active tab.
pub(crate) fn render(
    repo_paths: &[String],
    attention: &[bool],
    active: usize,
    area: Rect,
    accent: Color,
    attention_bright: bool,
) -> Paragraph<'static> {
    let spans: Vec<Span> = tab_segments(repo_paths, attention, active, area.width)
        .into_iter()
        .flat_map(|(text, index)| {
            // A `+N` marker is never the active tab, so accent stays a
            // reliable "this is the project you are in" signal.
            if index == active && !text.starts_with(" +") {
                return vec![Span::styled(
                    text,
                    Style::default()
                        .fg(Color::Black)
                        .bg(accent)
                        .add_modifier(Modifier::BOLD),
                )];
            }
            let base = if text.starts_with(" +") {
                Style::default().fg(Color::DarkGray)
            } else {
                Style::default().fg(Color::Gray)
            };
            let has_attention = if text.starts_with(" +") {
                text.ends_with(ATTENTION_GLYPH)
            } else {
                attention.get(index).copied().unwrap_or(false)
            };
            if !has_attention {
                return vec![Span::styled(text, base)];
            }

            let marker_start = text
                .find(ATTENTION_GLYPH)
                .expect("attention segment must contain its marker");
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
        })
        .collect();
    Paragraph::new(Line::from(spans))
}

/// The project index a click at screen cell `(x, y)` selects, or `None` off
/// the row or past the last tab. `area` is the tab row Rect.
pub(crate) fn tab_at(
    repo_paths: &[String],
    attention: &[bool],
    active: usize,
    area: Rect,
    x: u16,
    y: u16,
) -> Option<usize> {
    // On a terminal too short for the full chrome, ratatui hands the fixed tab
    // constraint a zero-height Rect and nothing is drawn. Without the size
    // check a click on whatever *is* visible at that y would select tab 0.
    if area.height == 0 || area.width == 0 || y != area.y || x < area.x {
        return None;
    }
    let mut cursor = area.x;
    for (text, index) in tab_segments(repo_paths, attention, active, area.width) {
        let width = Span::raw(text).width() as u16;
        if x < cursor.saturating_add(width) {
            return Some(index);
        }
        cursor = cursor.saturating_add(width);
    }
    None
}

#[cfg(test)]
mod tests;
