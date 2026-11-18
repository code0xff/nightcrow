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
        render_wrapped_rows(frame, inner, gutter_width, gutter, body);
        return;
    }
    let cols = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Length(gutter_width), Constraint::Min(0)])
        .split(inner);
    frame.render_widget(Paragraph::new(gutter), cols[0]);
    frame.render_widget(Paragraph::new(body).scroll((0, scroll_x)), cols[1]);
}

fn render_wrapped_rows(
    frame: &mut Frame,
    inner: Rect,
    gutter_width: u16,
    gutter: Vec<Line<'_>>,
    body: Vec<Line<'_>>,
) {
    let mut gutter = gutter.into_iter();
    let mut y = inner.top();
    for line in body {
        let remaining_height = inner.bottom().saturating_sub(y);
        if remaining_height == 0 {
            break;
        }

        // Headers and placeholders have no gutter contents and use the full width.
        let number = gutter.next().filter(|line| line.width() > 0);
        let reserved = if let Some(number) = number {
            let width = gutter_width.min(inner.width);
            frame.render_widget(number, Rect::new(inner.x, y, width, 1));
            width
        } else {
            0
        };
        let width = inner.width - reserved;
        let paragraph = Paragraph::new(line).wrap(Wrap { trim: false });
        let height = paragraph
            .line_count(width)
            .clamp(1, usize::from(remaining_height)) as u16;
        frame.render_widget(paragraph, Rect::new(inner.x + reserved, y, width, height));
        y += height;
    }
}
