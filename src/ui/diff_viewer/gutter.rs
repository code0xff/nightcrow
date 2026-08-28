use crate::git::diff::DiffHunk;
use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    text::Line,
    widgets::{Paragraph, Wrap},
};

/// Minimum digits reserved for one line-number column, so the gutter does not
/// twitch between a 1-digit and a 2-digit file.
const MIN_LINENO_DIGITS: usize = 3;

/// One padding space on each side of a number column: it lifts the digits off
/// the pane border and off the `+`/`-` marker that follows.
const LINENO_PAD: usize = 2;

/// Single space separating the old and new columns of the unified gutter.
const LINENO_GAP: usize = 1;

/// Digits needed to print `max_lineno`, floored at the minimum.
pub(crate) fn digits_for(max_lineno: usize) -> usize {
    let digits = if max_lineno == 0 {
        1
    } else {
        max_lineno.ilog10() as usize + 1
    };
    digits.max(MIN_LINENO_DIGITS)
}

/// Gutter digit count for a whole loaded diff: the widest line number that
/// appears on either side of any hunk. Derived from the loaded hunks, never
/// from the visible window, so scrolling cannot change the gutter width.
/// Recomputed per frame instead of cached: it is one allocation-free pass
/// over the same lines `ensure_highlight_cache` already walks.
pub(crate) fn lineno_digits(hunks: &[DiffHunk]) -> usize {
    let max = hunks
        .iter()
        .flat_map(|h| h.lines.iter())
        // `Option::max` picks the larger `Some`; both `None` only on fixtures
        // and the synthetic binary hunk, which then fall back to the minimum.
        .filter_map(|l| l.old_lineno.max(l.new_lineno))
        .max()
        .unwrap_or(0);
    digits_for(max as usize)
}

/// Width of the unified gutter, which shows the old and new columns together.
pub(crate) fn unified_gutter_width(digits: usize) -> u16 {
    (2 * digits + LINENO_GAP + LINENO_PAD) as u16
}

/// Width of a one-column gutter (each split half, and the file view).
pub(crate) fn side_gutter_width(digits: usize) -> u16 {
    (digits + LINENO_PAD) as u16
}

/// `" old new "`, with either column blank when the line is absent on that
/// side (added lines have no old number, removed lines have no new one).
pub(crate) fn unified_gutter_text(old: Option<u32>, new: Option<u32>, digits: usize) -> String {
    format!(
        " {:>digits$} {:>digits$} ",
        lineno_text(old),
        lineno_text(new)
    )
}

/// `" n "` for a single-column gutter; all spaces when `no` is `None`.
pub(crate) fn side_gutter_text(no: Option<u32>, digits: usize) -> String {
    format!(" {:>digits$} ", lineno_text(no))
}

fn lineno_text(no: Option<u32>) -> String {
    no.map(|v| v.to_string()).unwrap_or_default()
}

/// Render a pinned gutter column and a horizontally scrollable body inside
/// `inner` (a `Block`'s inner area — draw the block yourself first). They are
/// separate `Paragraph`s because `Paragraph::scroll` shifts the whole line, so
/// a gutter span in the body's paragraph would slide off the left edge;
/// vertical scroll is instead expressed by *which* lines the caller collected.
/// With `wrap` set that split is abandoned: a wrapped body line occupies
/// several screen rows while its gutter line still occupies one, so the number
/// is folded into the body line instead, where wrapping carries it along.
pub(crate) fn render_gutter_and_body(
    frame: &mut Frame,
    inner: Rect,
    gutter_width: u16,
    gutter: Vec<Line<'_>>,
    body: Vec<Line<'_>>,
    scroll_x: u16,
    wrap: bool,
) {
    if wrap {
        frame.render_widget(
            // `trim: false` keeps a continuation row's leading whitespace, which
            // in source code is the indentation.
            Paragraph::new(merge_gutter_into_body(gutter, body)).wrap(Wrap { trim: false }),
            inner,
        );
        return;
    }
    let cols = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Length(gutter_width), Constraint::Min(0)])
        .split(inner);
    frame.render_widget(Paragraph::new(gutter), cols[0]);
    frame.render_widget(Paragraph::new(body).scroll((0, scroll_x)), cols[1]);
}

/// Prepend each gutter line's spans to the body line it belongs to; the two
/// vectors are built in lockstep by the callers, so index `i` pairs row `i`.
fn merge_gutter_into_body<'a>(gutter: Vec<Line<'a>>, body: Vec<Line<'a>>) -> Vec<Line<'a>> {
    let mut gutter = gutter.into_iter();
    body.into_iter()
        .map(|line| match gutter.next() {
            Some(g) => {
                let mut spans = g.spans;
                spans.extend(line.spans);
                Line::from(spans)
            }
            None => line,
        })
        .collect()
}
