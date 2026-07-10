pub mod commit_list;
pub mod diff_pane;
pub mod diff_viewer;
pub mod file_list;
pub mod file_view;
pub mod log_view;
pub mod search;
pub mod splash;
pub mod status_view;
pub mod terminal_tab;
pub mod tree_list;
pub mod tree_view;

pub use search::SearchQuery;

use crate::app::{App, DiffPaneView, Focus, ViewMode};
use crate::config::LayoutConfig;
use crate::git::diff::StatusKind;
use crate::runtime::terminal::TerminalFullscreen;
use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Position, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem, ListState, Paragraph},
};
use syntect::highlighting::ThemeSet;
use syntect::parsing::SyntaxSet;

/// Extract a file path's extension as a `&str`, returning `""` when the path
/// has no extension or non-UTF-8 bytes. Shared by diff and file-view rendering
/// so syntax lookup behaves consistently regardless of the surface.
pub(crate) fn path_extension(path: &str) -> &str {
    std::path::Path::new(path)
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
}

pub(crate) fn focused_border_style(focused: bool, accent: Color) -> Style {
    if focused {
        Style::default().fg(accent)
    } else {
        Style::default().fg(Color::DarkGray)
    }
}

pub(crate) fn status_color(status: StatusKind) -> Color {
    match status {
        StatusKind::Added => Color::Green,
        StatusKind::Deleted => Color::Red,
        StatusKind::Renamed => Color::Cyan,
        StatusKind::TypeChanged => Color::Magenta,
        StatusKind::Unmerged => Color::Red,
        StatusKind::Untracked => Color::Gray,
        StatusKind::Modified => Color::Yellow,
        StatusKind::Unmodified => Color::DarkGray,
    }
}

/// Render a bordered, single-selection list with the project's standard
/// highlight styling. `selected` is clamped to `items.len() - 1` to match
/// the prior call sites' defensive behaviour.
pub(crate) fn render_selectable_list(
    frame: &mut Frame,
    area: Rect,
    title: String,
    items: Vec<ListItem<'_>>,
    selected: Option<usize>,
    border_style: Style,
) {
    let len = items.len();
    let list = List::new(items)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title(title)
                .border_style(border_style),
        )
        .highlight_style(
            Style::default()
                .bg(Color::DarkGray)
                .add_modifier(Modifier::BOLD),
        )
        .highlight_symbol("> ");

    let mut state = ListState::default();
    if len > 0
        && let Some(idx) = selected
    {
        state.select(Some(idx.min(len - 1)));
    }
    frame.render_stateful_widget(list, area, &mut state);
}

pub(crate) fn render_search_bar(
    frame: &mut Frame,
    query: &str,
    is_active: bool,
    area: Rect,
    accent: Color,
) {
    let cursor = if is_active { "█" } else { "" };
    let style = if is_active {
        Style::default().fg(accent)
    } else {
        Style::default().fg(Color::DarkGray)
    };
    frame.render_widget(
        Paragraph::new(format!("/{query}{cursor}")).style(style),
        area,
    );
}

fn main_content_constraints(layout: &LayoutConfig) -> [Constraint; 2] {
    [
        Constraint::Percentage(layout.upper_pct),
        Constraint::Percentage(100u16.saturating_sub(layout.upper_pct)),
    ]
}

/// Slice `s` past its first `scroll_x` characters, returning the remainder.
/// Used by the file/commit list renderers to scroll long entries horizontally
/// without slicing inside a multi-byte char boundary.
pub(crate) fn char_offset(s: &str, scroll_x: usize) -> &str {
    if scroll_x == 0 {
        return s;
    }
    let byte_off = s
        .char_indices()
        .nth(scroll_x)
        .map(|(b, _)| b)
        .unwrap_or(s.len());
    &s[byte_off..]
}

pub fn draw(
    frame: &mut Frame,
    app: &mut App,
    ss: &SyntaxSet,
    ts: &ThemeSet,
    layout: &LayoutConfig,
    accent: Color,
) {
    // Reserve 1 row at the top for the repo/branch header and 1 row at the
    // bottom for the hint/status bar. The header is rendered in every layout
    // branch (fullscreen included) so the repo identity is always visible.
    let outer = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1),
            Constraint::Min(0),
            Constraint::Length(1),
        ])
        .split(frame.area());
    let header_area = outer[0];
    let body_area = outer[1];
    let hint_area = outer[2];

    frame.render_widget(render_repo_header(app, accent), header_area);

    if app.terminal.fullscreen.fills_body() {
        terminal_tab::render(frame, app, body_area, accent);
        frame.render_widget(render_hint_bar(app, accent), hint_area);
        return;
    }

    if app.diff.fullscreen {
        diff_viewer::render(frame, app, body_area, ss, ts, accent);
        frame.render_widget(render_hint_bar(app, accent), hint_area);
        return;
    }

    if app.list_fullscreen {
        match app.mode {
            ViewMode::Status => file_list::render(frame, app, body_area, accent),
            ViewMode::Log => commit_list::render(frame, app, body_area, accent),
            ViewMode::Tree => tree_list::render(frame, app, body_area, accent),
        }
        frame.render_widget(render_hint_bar(app, accent), hint_area);
        return;
    }

    let main = Layout::default()
        .direction(Direction::Vertical)
        .constraints(main_content_constraints(layout))
        .split(body_area);

    let file_list_pct = layout.file_list_pct;
    let diff_pct = 100u16.saturating_sub(file_list_pct);
    let upper = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage(file_list_pct),
            Constraint::Percentage(diff_pct),
        ])
        .split(main[0]);

    match app.mode {
        ViewMode::Status => file_list::render(frame, app, upper[0], accent),
        ViewMode::Log => commit_list::render(frame, app, upper[0], accent),
        ViewMode::Tree => tree_list::render(frame, app, upper[0], accent),
    }
    diff_viewer::render(frame, app, upper[1], ss, ts, accent);
    terminal_tab::render(frame, app, main[1], accent);
    frame.render_widget(render_hint_bar(app, accent), hint_area);
}

/// Content Rect (post border) for every currently visible terminal pane,
/// keyed by pane id — `None` when the terminal panel isn't shown at all
/// (diff/list fullscreen). Used to resize each pane's PTY to exactly the
/// area `terminal_tab::render` draws it in.
pub(crate) fn terminal_content_areas(
    app: &App,
    screen_area: Rect,
    layout: &LayoutConfig,
) -> Vec<(crate::backend::PaneId, Rect)> {
    let Some(widget_area) = terminal_widget_area(app, screen_area, layout) else {
        return Vec::new();
    };
    terminal_tab::visible_pane_content_areas(app, widget_area)
}

/// The upper panel owning screen cell `(x, y)` in the normal split layout —
/// the click-to-focus hit test for the file/commit/tree list and the diff
/// viewer, mirroring `draw`'s geometry. `None` in every fullscreen state
/// (a body-filling panel already holds focus, and the terminal case belongs
/// to `pane_at`) and for cells on the header/hint rows or the terminal.
pub(crate) fn upper_panel_at(
    app: &App,
    screen_area: Rect,
    layout: &LayoutConfig,
    x: u16,
    y: u16,
) -> Option<Focus> {
    if app.terminal.fullscreen.fills_body() || app.diff.fullscreen || app.list_fullscreen {
        return None;
    }
    let outer = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1),
            Constraint::Min(0),
            Constraint::Length(1),
        ])
        .split(screen_area);
    let main = Layout::default()
        .direction(Direction::Vertical)
        .constraints(main_content_constraints(layout))
        .split(outer[1]);
    let file_list_pct = layout.file_list_pct;
    let upper = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage(file_list_pct),
            Constraint::Percentage(100u16.saturating_sub(file_list_pct)),
        ])
        .split(main[0]);

    let pos = Position { x, y };
    if upper[0].contains(pos) {
        Some(Focus::FileList)
    } else if upper[1].contains(pos) {
        Some(Focus::DiffViewer)
    } else {
        None
    }
}

/// The visible pane whose content rect contains screen cell `(x, y)`, with
/// that rect — the hit test for mouse events, using 0-based screen
/// coordinates as crossterm reports them. `None` when the cell lies outside
/// every pane's content (upper panels, borders, tab bar), or when the
/// terminal isn't drawn at all because another panel is fullscreen.
pub(crate) fn pane_at(
    app: &App,
    screen_area: Rect,
    layout: &LayoutConfig,
    x: u16,
    y: u16,
) -> Option<(crate::backend::PaneId, Rect)> {
    terminal_content_areas(app, screen_area, layout)
        .into_iter()
        .find(|(_, rect)| rect.contains(Position { x, y }))
}

/// The full terminal widget area (tab row + content), matching exactly what
/// `terminal_tab::render` is given as its `area` argument in `draw`. `None`
/// when a different pane is fullscreen and the terminal isn't drawn at all.
fn terminal_widget_area(app: &App, screen_area: Rect, layout: &LayoutConfig) -> Option<Rect> {
    let outer = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1),
            Constraint::Min(0),
            Constraint::Length(1),
        ])
        .split(screen_area);
    let body_area = outer[1];

    if app.terminal.fullscreen.fills_body() {
        return Some(body_area);
    }
    if app.diff.fullscreen || app.list_fullscreen {
        return None;
    }

    let main = Layout::default()
        .direction(Direction::Vertical)
        .constraints(main_content_constraints(layout))
        .split(body_area);
    Some(main[1])
}

/// Render the top header strip: `repo-path  branch  ↑N ↓M`. Branch and
/// tracking chips are omitted when their data is absent so the line stays
/// short on detached HEAD or empty repos.
fn render_repo_header<'a>(app: &'a App, accent: Color) -> Paragraph<'a> {
    let display_path = home_relative_path(&app.repo_path);
    let mut spans: Vec<Span<'a>> = vec![Span::styled(
        format!(" {display_path} "),
        Style::default()
            .fg(Color::Gray)
            .add_modifier(Modifier::BOLD),
    )];
    if let Some(branch) = app.branch_name.as_deref() {
        spans.push(Span::styled(
            format!(" {branch} "),
            Style::default().fg(accent).add_modifier(Modifier::BOLD),
        ));
    }
    if let Some(t) = &app.tracking
        && (t.ahead > 0 || t.behind > 0)
    {
        spans.push(Span::styled(
            format!(" ↑{} ↓{} ", t.ahead, t.behind),
            Style::default().fg(Color::Cyan),
        ));
    }
    Paragraph::new(Line::from(spans))
}

/// Replace the user's home prefix with `~` for display, leaving non-home
/// paths unchanged. Trailing path separator (libgit2 workdirs include one)
/// is stripped so the header stays compact.
fn home_relative_path(path: &str) -> String {
    let trimmed = path.trim_end_matches('/');
    if let Some(home) = dirs::home_dir()
        && let Some(home_str) = home.to_str()
        && let Some(rest) = trimmed.strip_prefix(home_str)
    {
        return format!("~{rest}");
    }
    trimmed.to_string()
}

fn render_hint_bar(app: &App, accent: Color) -> Paragraph<'_> {
    if app.repo_input.active {
        return Paragraph::new(Line::from(vec![
            Span::styled("repo: ", Style::default().fg(accent)),
            Span::raw(app.repo_input.buf.as_str()),
            Span::styled("█", Style::default().fg(accent)),
        ]));
    }
    if app.prefix_armed() {
        // While the terminal fills the body the digit row addresses panes
        // directly (`1-8`); in the split view `1`/`2` focus the list/diff and
        // `3-9,0` jump to panes (see `main::resolve_prefix_action`).
        let digits = if app.terminal.fullscreen.fills_body() {
            "1-8: pane"
        } else {
            "1-9: focus/pane"
        };
        return Paragraph::new(Line::from(vec![
            Span::styled(
                " PREFIX ",
                Style::default()
                    .fg(Color::Black)
                    .bg(accent)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                format!(" t: new pane | w: close | s: swap pane | l: log/status | b: tree/status | f: fullscreen | o: repo | p: theme | r: redraw | q: quit | {digits} | esc: cancel"),
                Style::default().fg(Color::DarkGray),
            ),
        ]));
    }
    if app.awaiting_swap_target() {
        // The swap-target digits follow the same layout-aware mapping as the
        // focus jumps (see `main::resolve_prefix_action`): `1-8` while the
        // terminal fills the body, `3-9,0` in the split view.
        let digits = if app.terminal.fullscreen.fills_body() {
            "1-8"
        } else {
            "3-9,0"
        };
        return Paragraph::new(Line::from(vec![
            Span::styled(
                " SWAP ",
                Style::default()
                    .fg(Color::Black)
                    .bg(accent)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                format!(" {digits}: swap active pane with this pane | esc: cancel"),
                Style::default().fg(Color::DarkGray),
            ),
        ]));
    }
    if let Some(ref msg) = app.status {
        return Paragraph::new(Line::from(msg.as_str())).style(Style::default().fg(Color::Red));
    }
    // `<prefix>` in the hint strings below is a single placeholder that resolves
    // to the configured leader chord (e.g. `^Q`) so the footer always names the
    // actual key to press rather than an abstract word.
    let leader = app.leader_label();
    let render = |hint: &str| {
        Paragraph::new(Line::from(Span::styled(
            hint.replace("<prefix>", &leader),
            Style::default().fg(Color::DarkGray),
        )))
    };
    match app.terminal.fullscreen {
        // From Grid the next `f` zooms the active pane — but only when Zoom
        // would look different from Grid; otherwise the cycle skips Zoom and
        // `f` exits.
        TerminalFullscreen::Grid if app.terminal.zoom_distinct_from_grid() => {
            return render(
                " <prefix>: leader | shift+↑/↓: scroll | shift+pgup/dn: page scroll | shift+←/→: cycle pane | <prefix> f: zoom active pane | <prefix> t: new pane | <prefix> w: close pane | <prefix> q: quit",
            );
        }
        TerminalFullscreen::Grid => {
            return render(
                " <prefix>: leader | shift+↑/↓: scroll | shift+pgup/dn: page scroll | <prefix> f: exit fullscreen | <prefix> t: new pane | <prefix> w: close pane | <prefix> q: quit",
            );
        }
        TerminalFullscreen::Zoom => {
            return render(
                " <prefix>: leader | shift+↑/↓: scroll | shift+pgup/dn: page scroll | shift+←/→: cycle pane | <prefix> f: exit fullscreen | <prefix> t: new pane | <prefix> w: close pane | <prefix> q: quit",
            );
        }
        TerminalFullscreen::Off => {}
    }
    if app.diff.fullscreen {
        let hint = if app.diff.view == DiffPaneView::File {
            " <prefix> f: exit zoom | v: back to diff | j/k: scroll | pgup/pgdn: page | <prefix> q: quit"
        } else if app.diff.view == DiffPaneView::Split {
            " <prefix> f: exit zoom | s: unified diff | j/k: scroll | pgup/pgdn: page | <prefix> q: quit"
        } else if app.diff.search.active {
            " type to search | enter: confirm | esc: cancel"
        } else if !app.diff.search.query.is_empty() {
            " <prefix> f: exit zoom | n: next match | shift+n: prev match | /: new search | esc: clear"
        } else {
            " <prefix> f: exit zoom | j/k: scroll | v: view file | s: split | /: search | pgup/pgdn: page | <prefix> q: quit"
        };
        return render(hint);
    }
    if app.list_fullscreen {
        let hint = match app.mode {
            ViewMode::Log if app.log_view.drill_down => {
                " <prefix> f: exit zoom | esc: back to commits | j/k: navigate files | <prefix> q: quit"
            }
            ViewMode::Log => {
                " <prefix> f: exit zoom | <prefix> l: status view | j/k: navigate commits | enter: view files | <prefix> q: quit"
            }
            ViewMode::Status => {
                " <prefix> f: exit zoom | j/k: navigate | /: search | <prefix> l: log view | <prefix> q: quit"
            }
            ViewMode::Tree => {
                " <prefix> f: exit zoom | j/k: navigate | /: search | →/enter: expand | ←: collapse | <prefix> b: status view | <prefix> q: quit"
            }
        };
        return render(hint);
    }
    if let Focus::Terminal = app.focus {
        return render(
            " <prefix>: leader | shift+↑/↓: scroll | shift+pgup/dn: page scroll | shift+←/→: cycle | <prefix> t: new pane | <prefix> w: close pane | <prefix> f: fullscreen | <prefix> l: log view | <prefix> o: repo | <prefix> q: quit",
        );
    }
    let hint = match app.focus {
        Focus::Terminal => unreachable!("Focus::Terminal handled above"),
        Focus::FileList => match app.mode {
            ViewMode::Log => {
                if app.log_view.drill_down {
                    " esc: back to commits | j/k: navigate files | shift+←/→: cycle | <prefix> q: quit"
                } else {
                    " shift+←/→: cycle | j/k: navigate commits | enter: view files | <prefix> t: new pane | <prefix> w: close pane | <prefix> f: fullscreen | <prefix> l: status view | <prefix> o: repo | <prefix> q: quit"
                }
            }
            ViewMode::Status => {
                " shift+←/→: cycle | j/k: navigate | /: search | <prefix> t: new pane | <prefix> w: close pane | <prefix> f: fullscreen | <prefix> l: log view | <prefix> b: tree view | <prefix> o: repo | <prefix> q: quit"
            }
            ViewMode::Tree => {
                " shift+←/→: cycle | j/k: navigate | /: search | →/enter: expand | ←: collapse | <prefix> b: status view | <prefix> l: log view | <prefix> q: quit"
            }
        },
        Focus::DiffViewer => {
            if app.diff.view == DiffPaneView::File && app.diff.search.active {
                " type to search | enter: confirm | esc: cancel"
            } else if app.diff.view == DiffPaneView::File && !app.diff.search.query.is_empty() {
                " n: next match | shift+n: prev match | /: new search | esc: clear"
            } else if app.diff.view == DiffPaneView::File {
                " v: back to diff | j/k: scroll | pgup/pgdn: page | /: search | shift+←/→: cycle | <prefix> q: quit"
            } else if app.diff.view == DiffPaneView::Split {
                " s: unified diff | j/k: scroll | pgup/pgdn: page | shift+←/→: cycle | <prefix> f: zoom | <prefix> q: quit"
            } else if app.diff.search.active {
                " type to search | enter: confirm | esc: cancel"
            } else if !app.diff.search.query.is_empty() {
                " n: next match | shift+n: prev match | /: new search | esc: clear"
            } else {
                " shift+←/→: cycle | j/k: scroll | pgup/pgdn: scroll | v: view file | s: split | /: search | <prefix> t: new pane | <prefix> w: close pane | <prefix> f: zoom | <prefix> l: log view | <prefix> o: repo | <prefix> q: quit"
            }
        }
    };
    render(hint)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::tests::{app_with_fake_backend, app_with_files};
    use crate::runtime::terminal::TerminalFullscreen;
    use ratatui::{Terminal, backend::TestBackend};

    /// Render the hint bar into a wide buffer and return its flattened text so
    /// footer wording can be asserted layout by layout.
    fn hint_text(app: &App) -> String {
        let mut terminal = Terminal::new(TestBackend::new(200, 1)).unwrap();
        terminal
            .draw(|frame| frame.render_widget(render_hint_bar(app, Color::Yellow), frame.area()))
            .unwrap();
        let buf = terminal.backend().buffer();
        (0..buf.area.height)
            .map(|y| {
                (0..buf.area.width)
                    .map(|x| buf[(x, y)].symbol())
                    .collect::<String>()
            })
            .collect::<Vec<_>>()
            .join("\n")
    }

    #[test]
    fn swap_hint_advertises_split_view_digits_by_default() {
        let mut app = app_with_fake_backend();
        app.begin_swap_target();

        assert!(
            hint_text(&app).contains("3-9,0: swap active pane"),
            "split view swap prompt must advertise the 3-9,0 mapping"
        );
    }

    #[test]
    fn swap_hint_advertises_fullscreen_digits_when_terminal_fills_body() {
        let mut app = app_with_fake_backend();
        app.terminal.fullscreen = TerminalFullscreen::Grid;
        app.begin_swap_target();

        let text = hint_text(&app);
        assert!(
            text.contains("1-8: swap active pane"),
            "fullscreen swap prompt must advertise the 1-8 mapping, got: {text}"
        );
        assert!(
            !text.contains("3-9,0"),
            "fullscreen swap prompt must not show the split-view digits, got: {text}"
        );
    }

    #[test]
    fn prefix_hint_switches_pane_digit_legend_by_layout() {
        let mut split = app_with_fake_backend();
        split.arm_prefix();
        assert!(
            hint_text(&split).contains("1-9: focus/pane"),
            "split view prefix hint must advertise focus/pane digits"
        );

        let mut full = app_with_fake_backend();
        full.terminal.fullscreen = TerminalFullscreen::Grid;
        full.arm_prefix();
        let text = hint_text(&full);
        assert!(
            text.contains("1-8: pane"),
            "fullscreen prefix hint must advertise the 1-8 pane digits, got: {text}"
        );
        assert!(
            !text.contains("1-9: focus/pane"),
            "fullscreen prefix hint must not show the split-view legend, got: {text}"
        );
    }

    #[test]
    fn home_relative_strips_home_prefix_and_trailing_slash() {
        let home = dirs::home_dir().expect("home dir for test host");
        let home_str = home.to_str().unwrap();
        let nested = format!("{home_str}/projects/foo/");
        assert_eq!(home_relative_path(&nested), "~/projects/foo");
    }

    #[test]
    fn home_relative_keeps_paths_outside_home_unchanged() {
        // Trailing slash still trimmed for compactness, but the body is
        // returned verbatim when the home prefix doesn't match.
        assert_eq!(home_relative_path("/tmp/repo/"), "/tmp/repo");
        assert_eq!(home_relative_path("/var/code"), "/var/code");
    }

    #[test]
    fn main_content_split_preserves_lower_panel_at_high_upper_ratio() {
        let cfg = LayoutConfig {
            upper_pct: 99,
            file_list_pct: 25,
        };

        assert_eq!(
            main_content_constraints(&cfg),
            [Constraint::Percentage(99), Constraint::Percentage(1)]
        );
    }

    #[test]
    fn terminal_content_areas_hidden_when_other_pane_is_fullscreen() {
        let mut app = app_with_files(vec!["a.rs"]);
        app.toggle_diff_fullscreen();

        let areas =
            terminal_content_areas(&app, Rect::new(0, 0, 100, 40), &LayoutConfig::default());

        assert!(areas.is_empty());
    }

    #[test]
    fn terminal_content_areas_uses_body_when_terminal_fullscreen() {
        let mut app = app_with_files(vec!["a.rs"]);
        app.terminal.panes.push(crate::app::PaneInfo {
            id: 1,
            title: "shell".to_string(),
        });
        app.toggle_terminal_fullscreen();

        let areas =
            terminal_content_areas(&app, Rect::new(0, 0, 100, 40), &LayoutConfig::default());

        // Full screen keeps the top header and bottom hint bar, then the
        // terminal widget consumes one tab row and the top/bottom border rows.
        // Side borders were dropped, so the content spans the full width. A
        // single pane has no per-cell border, so its content Rect equals the
        // whole terminal content area.
        assert_eq!(areas.len(), 1);
        assert_eq!(areas[0].0, 1);
        assert_eq!(areas[0].1.height, 35);
        assert_eq!(areas[0].1.width, 100);
    }

    #[test]
    fn pane_at_resolves_the_pane_under_a_cell_and_misses_elsewhere() {
        let mut app = app_with_files(vec!["a.rs"]);
        app.terminal.panes.push(crate::app::PaneInfo {
            id: 1,
            title: "shell".to_string(),
        });
        app.terminal.panes.push(crate::app::PaneInfo {
            id: 2,
            title: "shell".to_string(),
        });
        let screen = Rect::new(0, 0, 100, 40);
        let layout = LayoutConfig::default();
        let areas = terminal_content_areas(&app, screen, &layout);
        assert_eq!(areas.len(), 2);

        // A cell inside each pane's content rect resolves to that pane.
        for (id, rect) in &areas {
            let hit = pane_at(&app, screen, &layout, rect.x, rect.y);
            assert_eq!(hit, Some((*id, *rect)));
        }
        // The top-left corner belongs to the upper panels, not a pane.
        assert_eq!(pane_at(&app, screen, &layout, 0, 0), None);
    }

    #[test]
    fn upper_panel_at_resolves_list_and_diff_by_the_layout_split() {
        let app = app_with_files(vec!["a.rs"]);
        let screen = Rect::new(0, 0, 100, 40);
        let layout = LayoutConfig::default();

        // Row 0 is the repo header, row 1 the first body row. The default
        // file_list_pct (25) puts x=0 in the list and x=60 in the diff.
        assert_eq!(upper_panel_at(&app, screen, &layout, 0, 0), None);
        assert_eq!(
            upper_panel_at(&app, screen, &layout, 0, 1),
            Some(Focus::FileList)
        );
        assert_eq!(
            upper_panel_at(&app, screen, &layout, 60, 1),
            Some(Focus::DiffViewer)
        );
        // The last body row belongs to the terminal panel, the row after it
        // to the hint bar — neither is an upper panel.
        assert_eq!(upper_panel_at(&app, screen, &layout, 0, 38), None);
        assert_eq!(upper_panel_at(&app, screen, &layout, 0, 39), None);
    }

    #[test]
    fn upper_panel_at_misses_in_every_fullscreen_state() {
        // The implementation guards three distinct flags; each must miss on
        // its own, at a cell that hits the file list in the normal split.
        let screen = Rect::new(0, 0, 100, 40);
        let layout = LayoutConfig::default();

        let mut diff_full = app_with_files(vec!["a.rs"]);
        diff_full.toggle_diff_fullscreen();
        assert_eq!(upper_panel_at(&diff_full, screen, &layout, 0, 1), None);

        let mut list_full = app_with_files(vec!["a.rs"]);
        list_full.list_fullscreen = true;
        assert_eq!(upper_panel_at(&list_full, screen, &layout, 0, 1), None);

        let mut term_full = app_with_files(vec!["a.rs"]);
        term_full.terminal.fullscreen = TerminalFullscreen::Grid;
        assert_eq!(upper_panel_at(&term_full, screen, &layout, 0, 1), None);
    }

    #[test]
    fn pane_at_misses_when_another_panel_is_fullscreen() {
        let mut app = app_with_files(vec!["a.rs"]);
        app.terminal.panes.push(crate::app::PaneInfo {
            id: 1,
            title: "shell".to_string(),
        });
        app.toggle_diff_fullscreen();

        let hit = pane_at(&app, Rect::new(0, 0, 100, 40), &LayoutConfig::default(), 50, 30);

        assert_eq!(hit, None);
    }
}
