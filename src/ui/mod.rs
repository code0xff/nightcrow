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

pub use search::SearchQuery;

use crate::app::{App, DiffPaneView, Focus, ViewMode};
use crate::config::LayoutConfig;
use crate::git::diff::ChangeStatus;
use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
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

pub(crate) fn status_color(status: ChangeStatus) -> Color {
    match status {
        ChangeStatus::Added => Color::Green,
        ChangeStatus::Deleted => Color::Red,
        ChangeStatus::Renamed => Color::Cyan,
        ChangeStatus::Untracked => Color::Gray,
        ChangeStatus::Modified => Color::Yellow,
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

    if app.terminal.fullscreen {
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
    }
    diff_viewer::render(frame, app, upper[1], ss, ts, accent);
    terminal_tab::render(frame, app, main[1], accent);
    frame.render_widget(render_hint_bar(app, accent), hint_area);
}

pub(crate) fn terminal_content_area(
    app: &App,
    screen_area: Rect,
    layout: &LayoutConfig,
) -> Option<Rect> {
    let outer = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1),
            Constraint::Min(0),
            Constraint::Length(1),
        ])
        .split(screen_area);
    let body_area = outer[1];

    if app.terminal.fullscreen {
        return terminal_tab::content_area(body_area);
    }
    if app.diff.fullscreen || app.list_fullscreen {
        return None;
    }

    let main = Layout::default()
        .direction(Direction::Vertical)
        .constraints(main_content_constraints(layout))
        .split(body_area);
    terminal_tab::content_area(main[1])
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
        return Paragraph::new(Line::from(vec![
            Span::styled(
                " PREFIX ",
                Style::default()
                    .fg(Color::Black)
                    .bg(accent)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                " t: new pane | w: close | l: log/status | f: fullscreen | o: repo | p: theme | q: quit | 1-7: pane | esc: cancel",
                Style::default().fg(Color::DarkGray),
            ),
        ]));
    }
    if let Some(ref msg) = app.status {
        return Paragraph::new(Line::from(msg.as_str())).style(Style::default().fg(Color::Red));
    }
    if app.terminal.fullscreen {
        let leader = app.leader_label();
        let hint = format!(
            " {leader}: leader | shift+↑/↓: scroll | shift+pgup/dn: page scroll | shift+←/→: cycle pane | <prefix> f: exit fullscreen | <prefix> t: new pane | <prefix> w: close pane | <prefix> q: quit"
        );
        return Paragraph::new(Line::from(Span::styled(
            hint,
            Style::default().fg(Color::DarkGray),
        )));
    }
    if app.diff.fullscreen {
        let hint = if app.diff.view == DiffPaneView::File {
            " <prefix> f: exit zoom | v: back to diff | j/k: scroll | pgup/pgdn: page | <prefix> q: quit"
        } else if app.diff.search.active {
            " type to search | enter: confirm | esc: cancel"
        } else if !app.diff.search.query.is_empty() {
            " <prefix> f: exit zoom | n: next match | shift+n: prev match | /: new search | esc: clear"
        } else {
            " <prefix> f: exit zoom | j/k: scroll | v: view file | /: search | pgup/pgdn: page | <prefix> q: quit"
        };
        return Paragraph::new(Line::from(Span::styled(
            hint,
            Style::default().fg(Color::DarkGray),
        )));
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
        };
        return Paragraph::new(Line::from(Span::styled(
            hint,
            Style::default().fg(Color::DarkGray),
        )));
    }
    if let Focus::Terminal = app.focus {
        let leader = app.leader_label();
        let hint = format!(
            " {leader}: leader | shift+↑/↓: scroll | shift+pgup/dn: page scroll | shift+←/→: cycle | <prefix> t: new pane | <prefix> w: close pane | <prefix> f: fullscreen | <prefix> l: log view | <prefix> o: repo | <prefix> q: quit"
        );
        return Paragraph::new(Line::from(Span::styled(
            hint,
            Style::default().fg(Color::DarkGray),
        )));
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
                " shift+←/→: cycle | j/k: navigate | /: search | <prefix> t: new pane | <prefix> w: close pane | <prefix> f: fullscreen | <prefix> l: log view | <prefix> o: repo | <prefix> q: quit"
            }
        },
        Focus::DiffViewer => {
            if app.diff.view == DiffPaneView::File {
                " v: back to diff | j/k: scroll | pgup/pgdn: page | shift+←/→: cycle | <prefix> q: quit"
            } else if app.diff.search.active {
                " type to search | enter: confirm | esc: cancel"
            } else if !app.diff.search.query.is_empty() {
                " n: next match | shift+n: prev match | /: new search | esc: clear"
            } else {
                " shift+←/→: cycle | j/k: scroll | pgup/pgdn: scroll | v: view file | /: search | <prefix> t: new pane | <prefix> w: close pane | <prefix> f: zoom | <prefix> l: log view | <prefix> o: repo | <prefix> q: quit"
            }
        }
    };
    Paragraph::new(Line::from(Span::styled(
        hint,
        Style::default().fg(Color::DarkGray),
    )))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::tests::app_with_files;

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
    fn terminal_content_area_hidden_when_other_pane_is_fullscreen() {
        let mut app = app_with_files(vec!["a.rs"]);
        app.toggle_diff_fullscreen();

        let area = terminal_content_area(&app, Rect::new(0, 0, 100, 40), &LayoutConfig::default());

        assert!(area.is_none());
    }

    #[test]
    fn terminal_content_area_uses_body_when_terminal_fullscreen() {
        let mut app = app_with_files(vec!["a.rs"]);
        app.terminal.panes.push(crate::app::PaneInfo {
            id: 1,
            title: "shell".to_string(),
        });
        app.toggle_terminal_fullscreen();

        let area = terminal_content_area(&app, Rect::new(0, 0, 100, 40), &LayoutConfig::default())
            .expect("terminal fullscreen should produce a content area");

        // Full screen keeps the top header and bottom hint bar, then the
        // terminal widget consumes one border row on each side and one tab row.
        assert_eq!(area.height, 35);
        assert_eq!(area.width, 98);
    }
}
