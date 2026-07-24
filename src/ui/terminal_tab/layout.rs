use crate::runtime::terminal::MAX_VISIBLE_FULLSCREEN;
use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    widgets::{Block, Borders},
};

/// The terminal pane draws only top/bottom borders, never the left/right `│`.
/// With side bars, selecting terminal output to copy picks up a `│` glyph on
/// every wrapped row; dropping them lets the content run edge-to-edge so a
/// copy is clean. Top stays for the title + focus tint, bottom for separation.
pub(crate) const TERMINAL_BORDERS: Borders = Borders::TOP.union(Borders::BOTTOM);

/// Per-tab character budget for the title (excluding the jump-key hint and
/// surrounding padding). Anything longer is truncated with a trailing ellipsis
/// so long OSC-set titles can't push neighboring tabs off the row.
pub(crate) const TAB_TITLE_MAX_CHARS: usize = 20;

/// Number of panes reachable by a leader-digit jump key. Panes past
/// this index have no jump-key hint in the tab bar (only focus cycling
/// reaches them). Tied to `MAX_VISIBLE_FULLSCREEN` by reference (not just by
/// convention) so the two can never silently drift apart.
pub(crate) const JUMP_KEY_PANE_COUNT: usize = MAX_VISIBLE_FULLSCREEN;

/// Truncate `title` to at most `max` characters, appending `…` when cut.
/// Char-based (not display-width) for simplicity: ASCII shell program names
/// are the common case and `chars().count()` is already correct there. CJK
/// titles render slightly under the visual budget, which is acceptable.
pub(crate) fn truncate_tab_title(title: &str, max: usize) -> String {
    if title.chars().count() <= max {
        return title.to_string();
    }
    // Reserve one char of the budget for the ellipsis itself.
    let keep = max.saturating_sub(1);
    let mut out: String = title.chars().take(keep).collect();
    out.push('…');
    out
}

pub(crate) fn terminal_layout(area: Rect) -> Option<(Rect, Rect)> {
    let inner = Block::default().borders(TERMINAL_BORDERS).inner(area);
    if inner.height == 0 || inner.width == 0 {
        return None;
    }
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(1), Constraint::Min(0)])
        .split(inner);
    Some((chunks[0], chunks[1]))
}

/// Split `area` into `count` cells using a balanced grid: 1 pane fills the
/// area; 2 panes go side by side when `area` is wide, stacked otherwise; 3
/// panes get a 2-column row plus a full-width remainder row; 4 is a 2x2
/// grid; 5-6 use 3 columns; 7 uses a 4-then-3 row split; 8 is a 2x4 grid.
/// Counts beyond that (not expected given `MAX_VISIBLE_FULLSCREEN`) fall back
/// to a near-square grid. Every returned Rect has at least 1x1 size when
/// `area` is at least `count` cells large, so no cell silently disappears.
pub(crate) fn split_pane_areas(area: Rect, count: usize) -> Vec<Rect> {
    if count == 0 || area.width == 0 || area.height == 0 {
        return Vec::new();
    }
    let plan = grid_row_plan(count, area);
    split_by_row_plan(area, &plan)
}

/// One entry per row, each entry the number of columns in that row.
fn grid_row_plan(count: usize, area: Rect) -> Vec<usize> {
    match count {
        1 => vec![1],
        2 => {
            if area.width >= area.height.saturating_mul(2) {
                vec![2]
            } else {
                vec![1, 1]
            }
        }
        3 => vec![2, 1],
        4 => vec![2, 2],
        5 => vec![3, 2],
        6 => vec![3, 3],
        7 => vec![4, 3],
        8 => vec![4, 4],
        n => {
            let cols = (n as f64).sqrt().ceil() as usize;
            let rows = n.div_ceil(cols);
            let mut plan = vec![cols; rows];
            let mut excess = cols * rows - n;
            let mut i = plan.len();
            while excess > 0 && i > 0 {
                i -= 1;
                let take = plan[i].saturating_sub(1).min(excess);
                plan[i] -= take;
                excess -= take;
            }
            plan.retain(|&c| c > 0);
            plan
        }
    }
}

fn split_by_row_plan(area: Rect, plan: &[usize]) -> Vec<Rect> {
    if plan.is_empty() {
        return Vec::new();
    }
    let row_constraints: Vec<Constraint> = plan.iter().map(|_| Constraint::Min(1)).collect();
    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints(row_constraints)
        .split(area);

    let mut result = Vec::with_capacity(plan.iter().sum());
    for (row_area, &cols) in rows.iter().zip(plan.iter()) {
        if cols == 0 {
            continue;
        }
        let col_constraints: Vec<Constraint> = (0..cols).map(|_| Constraint::Min(1)).collect();
        let cells = Layout::default()
            .direction(Direction::Horizontal)
            .constraints(col_constraints)
            .split(*row_area);
        result.extend(cells.iter().copied());
    }
    result
}
