//! The project tab row across the top of the screen.
//!
//! Mirrors `terminal_tab`'s tab bar deliberately: one `tab_segments` builder
//! feeding both the renderer and the click hit-test, so a label and its click
//! box can never disagree. The two rows differ only in what they address —
//! panes below, projects above — and in their key legends, which come from
//! different axes (leader digits for panes, bare F-keys for projects).

use ratatui::{
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::Paragraph,
};

/// Per-tab character budget for the project name. Shorter than the pane
/// budget: up to ten tabs share one row, where panes cap at eight and carry
/// shorter titles.
const TAB_TITLE_MAX_CHARS: usize = 14;

/// Width of a `+N` overflow marker. Exactly one digit is enough: at most
/// `MAX_PROJECTS` (10) tabs exist, so at most 9 can be hidden on one side.
const MARKER_WIDTH: u16 = 4;

/// The name shown for a repo path — its final component, which is what
/// distinguishes sibling checkouts (`~/work/api` vs `~/work/web`).
///
/// Goes through `Path` rather than splitting on `/` so a Windows path
/// (`C:\work\api`) yields `api` too; splitting by hand would render the whole
/// path there and waste the tab's width. Falls back to the path itself when it
/// has no final component (a filesystem root), and to a placeholder when it is
/// empty — a blank tab would look unclickable.
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

/// Truncate to at most `max` characters, appending `…` when cut. Char-based
/// rather than display-width, matching `terminal_tab::truncate_tab_title`.
fn truncate(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        return s.to_string();
    }
    let mut out: String = s.chars().take(max.saturating_sub(1)).collect();
    out.push('…');
    out
}

/// The full text of every tab, ignoring how many will fit.
///
/// Every tab carries its `F#` legend because the F-key row addresses projects
/// directly and layout-independently — unlike panes, whose digit legend shifts
/// with the layout. Projects past the tenth have no key, so they carry no
/// legend rather than implying an unbound one.
fn tab_texts(repo_paths: &[String]) -> Vec<String> {
    repo_paths
        .iter()
        .enumerate()
        .map(|(i, path)| {
            let name = tab_label(path);
            match i.checked_add(1).filter(|n| *n <= 10) {
                Some(n) => format!(" F{n} {name} "),
                None => format!(" {name} "),
            }
        })
        .collect()
}

/// The run of tabs to draw in `width` cells, always containing `active`.
///
/// Ten tabs of repo names do not fit an 80-column row, and a `Paragraph` would
/// simply clip the tail — silently hiding later projects *and* the active-tab
/// highlight when the active one falls off the end. So the row scrolls around
/// the active tab instead, and what is dropped is replaced by a `+N` marker
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

    // Grow right first, then left, until neither side can take another tab.
    // Right-first keeps the common case (active near the front) showing the
    // projects that follow it.
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
/// match what is on screen.
///
/// A `+N` marker selects the nearest project hidden on its side, so the
/// overflow is reachable by pointer as well as by F-key.
fn tab_segments(repo_paths: &[String], active: usize, width: u16) -> Vec<(String, usize)> {
    let texts = tab_texts(repo_paths);
    let widths: Vec<u16> = texts.iter().map(|t| Span::raw(t).width() as u16).collect();
    let visible = visible_window(&widths, width, active);

    let mut segments = Vec::with_capacity(visible.len() + 2);
    if visible.start > 0 {
        segments.push((format!(" +{} ", visible.start), visible.start - 1));
    }
    segments.extend(
        texts[visible.clone()]
            .iter()
            .enumerate()
            .map(|(offset, text)| (text.clone(), visible.start + offset)),
    );
    let hidden_after = texts.len() - visible.end;
    if hidden_after > 0 {
        segments.push((format!(" +{hidden_after} "), visible.end));
    }
    segments
}

/// Draw the tab row into `area`.
///
/// A single project still renders its tab: the row is permanent (see
/// `chrome_rows`), and showing which repo is open is exactly what the row is
/// for. `accent` marks the active tab, matching the app-wide convention that
/// accent means "this is the one in play".
pub(crate) fn render(
    repo_paths: &[String],
    active: usize,
    area: Rect,
    accent: Color,
) -> Paragraph<'static> {
    let spans: Vec<Span> = tab_segments(repo_paths, active, area.width)
        .into_iter()
        .map(|(text, index)| {
            // A `+N` marker is never the active tab, so accent stays a
            // reliable "this is the project you are in" signal.
            let style = if index == active && !text.starts_with(" +") {
                Style::default()
                    .fg(Color::Black)
                    .bg(accent)
                    .add_modifier(Modifier::BOLD)
            } else if text.starts_with(" +") {
                Style::default().fg(Color::DarkGray)
            } else {
                Style::default().fg(Color::Gray)
            };
            Span::styled(text, style)
        })
        .collect();
    Paragraph::new(Line::from(spans))
}

/// The project index a click at screen cell `(x, y)` selects, or `None` off
/// the row or past the last tab. `area` is the tab row Rect.
pub(crate) fn tab_at(
    repo_paths: &[String],
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
    for (text, index) in tab_segments(repo_paths, active, area.width) {
        let width = Span::raw(text).width() as u16;
        if x < cursor.saturating_add(width) {
            return Some(index);
        }
        cursor = cursor.saturating_add(width);
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::{Terminal, backend::TestBackend};

    fn paths(v: &[&str]) -> Vec<String> {
        v.iter().map(|s| s.to_string()).collect()
    }

    fn rendered_at(repo_paths: &[String], active: usize, width: u16) -> String {
        let mut terminal = Terminal::new(TestBackend::new(width, 1)).unwrap();
        terminal
            .draw(|frame| {
                frame.render_widget(
                    render(repo_paths, active, frame.area(), Color::Yellow),
                    frame.area(),
                );
            })
            .unwrap();
        let buf = terminal.backend().buffer();
        (0..buf.area.width)
            .map(|x| buf[(x, 0)].symbol())
            .collect::<String>()
    }

    fn rendered(repo_paths: &[String], active: usize) -> String {
        rendered_at(repo_paths, active, 120)
    }

    #[test]
    fn tab_label_uses_the_final_path_component() {
        assert_eq!(tab_label("/home/u/work/api"), "api");
        assert_eq!(tab_label("/home/u/work/api/"), "api");
    }

    #[test]
    fn tab_label_handles_root_and_empty_paths() {
        // A blank tab would look unclickable, so both degenerate cases still
        // produce a visible name.
        assert_eq!(tab_label("/"), "/");
        assert_eq!(tab_label(""), "?");
    }

    #[test]
    fn tab_label_truncates_a_long_name_with_an_ellipsis() {
        let label = tab_label("/w/a-very-long-project-name-here");

        assert_eq!(label.chars().count(), TAB_TITLE_MAX_CHARS);
        assert!(label.ends_with('…'), "got: {label}");
    }

    #[test]
    fn every_tab_carries_its_f_key_legend() {
        let text = rendered(&paths(&["/w/api", "/w/web"]), 0);

        assert!(text.contains("F1 api"), "got: {text}");
        assert!(text.contains("F2 web"), "got: {text}");
    }

    #[test]
    fn a_project_past_the_tenth_carries_no_key_legend() {
        // Only ten F-keys exist, so an eleventh tab must not imply one.
        let many: Vec<String> = (0..11).map(|i| format!("/w/p{i}")).collect();

        let segments = tab_segments(&many, 0, 240);

        assert_eq!(segments[9].0, " F10 p9 ");
        assert_eq!(segments[10].0, " p10 ");
    }

    /// Ten tabs whose names are long enough that the row cannot hold them all
    /// at 80 columns — the case a plain `Paragraph` would silently clip.
    fn crowded() -> Vec<String> {
        (0..10).map(|i| format!("/w/project-name-{i}")).collect()
    }

    #[test]
    fn a_crowded_row_stays_within_its_width() {
        for active in [0usize, 5, 9] {
            let segments = tab_segments(&crowded(), active, 80);
            let total: u16 = segments
                .iter()
                .map(|(t, _)| Span::raw(t).width() as u16)
                .sum();

            assert!(
                total <= 80,
                "active={active} overflowed the row: {total} cells"
            );
        }
    }

    #[test]
    fn the_active_tab_is_always_visible_however_crowded() {
        // Clipping the tail would hide the active tab — and with it the only
        // indication of which project the screen belongs to.
        for active in [0usize, 4, 9] {
            let text = rendered_at(&crowded(), active, 80);
            let expected = format!("F{} ", active + 1);

            assert!(
                text.contains(&expected),
                "active tab {active} must be on screen, got: {text}"
            );
        }
    }

    #[test]
    fn hidden_tabs_are_reported_by_overflow_markers() {
        // Scrolled to the far end, everything before it is behind one marker.
        let segments = tab_segments(&crowded(), 9, 80);

        let (marker, target) = &segments[0];
        assert!(marker.starts_with(" +"), "got: {marker}");
        // The marker selects the nearest hidden project, so the overflow is
        // reachable by pointer and not only by F-key.
        assert_eq!(
            *target,
            marker
                .trim()
                .trim_start_matches('+')
                .parse::<usize>()
                .unwrap()
                - 1
        );
    }

    #[test]
    fn a_row_that_fits_shows_no_markers() {
        let segments = tab_segments(&paths(&["/w/api", "/w/web"]), 0, 80);

        assert_eq!(segments.len(), 2, "no marker when everything fits");
    }

    #[test]
    fn only_the_active_tab_is_accented() {
        let repo_paths = paths(&["/w/api", "/w/web"]);
        let mut terminal = Terminal::new(TestBackend::new(120, 1)).unwrap();
        terminal
            .draw(|frame| {
                frame.render_widget(
                    render(&repo_paths, 1, frame.area(), Color::Yellow),
                    frame.area(),
                );
            })
            .unwrap();

        let buf = terminal.backend().buffer();
        let accented: String = (0..buf.area.width)
            .filter(|&x| buf[(x, 0)].style().bg == Some(Color::Yellow))
            .map(|x| buf[(x, 0)].symbol())
            .collect();

        assert!(accented.contains("web"), "got: {accented}");
        assert!(!accented.contains("api"), "got: {accented}");
    }

    #[test]
    fn tab_at_maps_a_click_to_the_tab_under_it() {
        let repo_paths = paths(&["/w/api", "/w/web"]);
        let area = Rect::new(0, 0, 120, 1);
        // Walk the rendered row rather than trusting the builder alone: the
        // hit box must match the glyphs actually on screen.
        let text = rendered(&repo_paths, 0);
        let web_x = text.find("F2 web").expect("second tab rendered") as u16;

        assert_eq!(tab_at(&repo_paths, 0, area, 0, 0), Some(0));
        assert_eq!(tab_at(&repo_paths, 0, area, web_x, 0), Some(1));
    }

    #[test]
    fn tab_at_is_none_off_the_row_and_past_the_last_tab() {
        let repo_paths = paths(&["/w/api"]);
        let area = Rect::new(0, 0, 120, 1);

        assert_eq!(tab_at(&repo_paths, 0, area, 0, 1), None, "wrong row");
        assert_eq!(tab_at(&repo_paths, 0, area, 100, 0), None, "past last tab");

        // A layout too short to give the row any cells must not report hits:
        // whatever is drawn at that y belongs to another row.
        let collapsed = Rect::new(0, 0, 120, 0);
        assert_eq!(tab_at(&repo_paths, 0, collapsed, 0, 0), None);
    }
}
