//! Which tabs fit, and what stands in for the ones that do not.
//!
//! The same arithmetic serves both placements. A row spends cells across and a
//! column spends rows down, so a tab's *extent* is its display width in the one
//! and exactly one in the other; the window that must always contain the active
//! tab, and the `+N` markers that hold the hidden tabs' places, are otherwise
//! the same shape whichever way they run.

use super::{ATTENTION_GLYPH, tab_label};
use crate::config::TabStrip;
use ratatui::text::Span;

/// Cells a `+N` marker takes in a row. In a column it is a row like any tab.
pub(super) const ROW_MARKER_WIDTH: u16 = 4;

/// The full text of every tab, ignoring how many will fit. Every tab carries
/// its `F#` legend because the F-key row addresses projects directly and
/// layout-independently; projects past the tenth have no key, so they carry
/// no legend rather than implying an unbound one.
pub(super) fn tab_texts(repo_paths: &[String], attention: &[bool]) -> Vec<String> {
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

/// The run of tabs to draw within `budget`, always containing `active`.
/// A `Paragraph` would silently clip the tail — hiding later projects *and*
/// the active-tab highlight — so the strip scrolls around the active tab and
/// drops what doesn't fit into `+N` markers whose extent is reserved first.
fn visible_window(
    extents: &[u16],
    budget: u16,
    active: usize,
    marker: u16,
) -> std::ops::Range<usize> {
    let n = extents.len();
    if n == 0 {
        return 0..0;
    }
    let active = active.min(n - 1);
    let (mut lo, mut hi) = (active, active + 1);
    let mut used = extents[active];

    // Cost of the window if it were [lo, hi): its tabs plus a marker on each
    // side that still has something hidden behind it.
    let fits = |used: u16, lo: usize, hi: usize| {
        let markers = (lo > 0) as u16 + (hi < n) as u16;
        used.saturating_add(markers * marker) <= budget
    };

    // Grow right first, then left: right-first keeps the common case (active
    // near the front) showing the projects that follow it.
    loop {
        let mut grew = false;
        if hi < n && fits(used + extents[hi], lo, hi + 1) {
            used += extents[hi];
            hi += 1;
            grew = true;
        }
        if lo > 0 && fits(used + extents[lo - 1], lo - 1, hi) {
            lo -= 1;
            used += extents[lo];
            grew = true;
        }
        if !grew {
            return lo..hi;
        }
    }
}

/// Build the strip's segments: rendered text paired with the project each one
/// selects. Single source for `render` and `tab_at`, so the hit boxes always
/// match what is on screen. A `+N` marker selects the nearest project hidden
/// on its side, so the overflow is reachable by pointer as well as by F-key.
///
/// `budget` is the cells across for a row and the rows down for a column.
pub(super) fn tab_segments(
    repo_paths: &[String],
    attention: &[bool],
    active: usize,
    budget: u16,
    strip: TabStrip,
) -> Vec<(String, usize)> {
    let texts = tab_texts(repo_paths, attention);
    let (extents, marker): (Vec<u16>, u16) = match strip {
        TabStrip::Top => (
            texts.iter().map(|t| Span::raw(t).width() as u16).collect(),
            ROW_MARKER_WIDTH,
        ),
        TabStrip::Left => (vec![1; texts.len()], 1),
    };
    let visible = visible_window(&extents, budget, active, marker);

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
