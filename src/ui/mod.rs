pub mod commit_list;
pub mod diff_pane;
pub mod diff_viewer;
pub mod file_list;
pub mod file_view;
pub mod log_view;
pub mod project_tab;
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
use crate::ui::status_view::RepoInput;
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
/// Key legend for a panel or pane reached by a leader digit, e.g. `^F1`.
///
/// Panels advertise the chord that actually reaches them. The bare F-key row
/// used to serve here, but it now selects project tabs, so a label reading
/// `F1 Files` would name a key that switches projects instead.
pub(crate) fn jump_legend(app: &App, digit: char) -> String {
    format!("{}{}", app.leader_label(), digit)
}

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

/// Split the screen into the four top-level rows: the project tab row, the
/// body, the notice row (repo identity, or a notice covering it), and the
/// hint bar.
///
/// The bottom chrome sits at the bottom so a rejected repo path lands directly
/// above the input the user has to correct, instead of across the screen from
/// it. The project tabs go on top instead, where tab rows are conventionally
/// looked for, and because they name the thing the whole screen belongs to
/// rather than commenting on the input at the bottom.
///
/// The tab row is permanent, not toggled by tab count. A row that came and
/// went would resize every PTY each time a project opened or closed — the same
/// churn that keeps the notice row an overlay rather than a row of its own.
/// Holding it always costs one SIGWINCH per pane at startup and none after.
///
/// This is called from `draw` and from three geometry helpers that must land on
/// exactly the same cells — the PTY sizer, the upper-panel hit test, and the
/// hint-bar hit test. They were four hand-copied splits before; one drifting
/// from the others mis-sizes terminals or offsets every mouse click by a row,
/// so the split lives here only.
fn chrome_rows(screen_area: Rect) -> ChromeRows {
    let outer = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1),
            Constraint::Min(0),
            Constraint::Length(1),
            Constraint::Length(1),
        ])
        .split(screen_area);
    ChromeRows {
        tabs: outer[0],
        body: outer[1],
        notice: outer[2],
        hint: outer[3],
    }
}

/// The four top-level rows. Named rather than a tuple because four same-typed
/// Rects are too easy to mis-order at a call site.
struct ChromeRows {
    tabs: Rect,
    body: Rect,
    notice: Rect,
    hint: Rect,
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

/// The process-level state the chrome draws, alongside the active project.
///
/// Passed as data rather than as the `Workspace` itself: rendering reads one
/// project plus this summary, and taking the whole workspace would hand every
/// renderer access to projects it must not touch.
#[derive(Clone, Copy)]
pub struct Chrome<'a> {
    pub repo_paths: &'a [String],
    pub active: usize,
    /// The open-repo dialog, which lives on the workspace because it must work
    /// with no project open.
    pub repo_input: &'a RepoInput,
}

/// The project index a click at `(x, y)` selects, or `None` when the click is
/// not on a project tab. Shares `chrome_rows` with `draw`, so the hit boxes
/// track the rendered row.
pub(crate) fn project_tab_at(tabs: Chrome<'_>, screen_area: Rect, x: u16, y: u16) -> Option<usize> {
    project_tab::tab_at(
        tabs.repo_paths,
        tabs.active,
        chrome_rows(screen_area).tabs,
        x,
        y,
    )
}

/// The empty screen's hint legend, unarmed and armed. Shared by the renderer
/// and the click hit-test so a clickable label and its target cannot drift —
/// the armed row is laid out differently (chip plus bare keys), so measuring
/// the wrong one would leave parts of a rendered label unclickable.
const EMPTY_HINT: &str = " <prefix> o: open project | <prefix> q: quit";
const EMPTY_HINT_ARMED: &str = " o: open project | q: quit | esc: cancel";

/// The click action for `(x, y)` on the empty screen's hint row, or `None`
/// off it. Only `o` resolves — quitting stays a deliberate keyboard act, as
/// on the project screen.
pub(crate) fn empty_hint_click_at(
    screen_area: Rect,
    leader_label: &str,
    prefix_armed: bool,
    mouse_enabled: bool,
    x: u16,
    y: u16,
) -> Option<HintClick> {
    // Gated like `hint_click_at`: with capture off the row renders plain, and
    // a browser mouse event still reaches this path, so a label that does not
    // advertise itself as clickable must not act like one either.
    if !mouse_enabled {
        return None;
    }
    let hint_area = chrome_rows(screen_area).hint;
    if hint_area.height == 0 || !hint_area.contains(Position { x, y }) {
        return None;
    }
    let (chip, text) = if prefix_armed {
        (PREFIX_CHIP, EMPTY_HINT_ARMED)
    } else {
        ("", EMPTY_HINT)
    };
    let mut cursor = hint_area.x + Span::raw(chip).width() as u16;
    for (i, segment) in text.split(" | ").enumerate() {
        if i > 0 {
            cursor += Span::raw(" | ").width() as u16;
        }
        let rendered = segment.replace("<prefix>", leader_label);
        let width = Span::raw(rendered.as_str()).width() as u16;
        if x >= cursor && x < cursor + width {
            // Same rules as `hint_click_at`: leading whitespace renders plain
            // and so is not part of the target, and the key is the text before
            // the colon.
            let label_start = rendered.len() - rendered.trim_start().len();
            let lead_width = Span::raw(&rendered[..label_start]).width() as u16;
            if x < cursor + lead_width {
                return None;
            }
            let (keyspec, _) = segment.split_once(':')?;
            return segment_click(keyspec);
        }
        cursor += width;
    }
    None
}

/// Render the screen with no project open.
///
/// The body is a placeholder rather than a borrowed panel: every viewer here
/// reads a repo, and there is none. The chrome still draws — an empty tab row,
/// the notice row (which carries a rejected path from the open dialog, since
/// repo identity has nothing to show), and a hint bar naming the only two
/// things that work from here.
pub fn draw_empty(
    frame: &mut Frame,
    chrome: Chrome<'_>,
    notice: Option<&crate::app::Notice>,
    leader: crossterm::event::KeyEvent,
    prefix_armed: bool,
    mouse_enabled: bool,
    accent: Color,
) {
    let rows = chrome_rows(frame.area());
    frame.render_widget(
        project_tab::render(chrome.repo_paths, chrome.active, rows.tabs, accent),
        rows.tabs,
    );

    let leader_label = crate::app::leader_label_of(leader);
    frame.render_widget(
        Paragraph::new(Line::from(vec![Span::styled(
            format!("  no project open — {leader_label} o to open a repo"),
            Style::default().fg(Color::DarkGray),
        )]))
        .block(Block::default().borders(Borders::ALL)),
        rows.body,
    );

    // Matches `render_notice_row`: a notice is the same red wherever it lands.
    let notice_line = match notice {
        Some(n) => Line::from(Span::styled(
            format!(" {}", n.line()),
            Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
        )),
        None => Line::default(),
    };
    frame.render_widget(Paragraph::new(notice_line), rows.notice);

    // The armed prefix shows the same chip as the project screen: pressing the
    // leader here has to look like it did something, or it reads as a dead key.
    let hint = if chrome.repo_input.active {
        Line::from(vec![
            Span::styled("repo: ", Style::default().fg(accent)),
            Span::raw(chrome.repo_input.buf.clone()),
            Span::styled("█", Style::default().fg(accent)),
        ])
    } else if prefix_armed {
        let mut spans = vec![Span::styled(
            PREFIX_CHIP,
            Style::default()
                .fg(Color::Black)
                .bg(accent)
                .add_modifier(Modifier::BOLD),
        )];
        spans.extend(hint_spans(EMPTY_HINT_ARMED, &leader_label, mouse_enabled));
        Line::from(spans)
    } else {
        Line::from(hint_spans(EMPTY_HINT, &leader_label, mouse_enabled))
    };
    frame.render_widget(Paragraph::new(hint), rows.hint);
}

/// Render one frame, returning the screen cell the terminal cursor was placed
/// on (`None` when no cursor is shown). Ratatui applies the cursor to the local
/// terminal itself, but the web mirror streams only the cell buffer, so the
/// position has to be handed back for `web::WebServer::broadcast` to replay.
pub fn draw(
    frame: &mut Frame,
    app: &mut App,
    tabs: Chrome<'_>,
    ss: &SyntaxSet,
    ts: &ThemeSet,
    layout: &LayoutConfig,
    accent: Color,
) -> Option<Position> {
    // Chrome: the project tab row on top, the notice row (repo identity, or a
    // notice covering it) and the hint bar below. The tab row and notice row
    // are rendered here, before any layout branch, so neither is lost to a
    // fullscreen view mode — a tab row that vanished in fullscreen would strand
    // the user with no indication of which project they are in.
    let rows = chrome_rows(frame.area());
    let (body_area, notice_area, hint_area) = (rows.body, rows.notice, rows.hint);

    frame.render_widget(
        project_tab::render(tabs.repo_paths, tabs.active, rows.tabs, accent),
        rows.tabs,
    );
    frame.render_widget(render_notice_row(app, accent), notice_area);

    if app.terminal.fullscreen.fills_body() {
        let cursor = terminal_tab::render(frame, app, body_area, accent);
        frame.render_widget(render_hint_bar(app, tabs, accent), hint_area);
        return cursor;
    }

    if app.diff.fullscreen {
        diff_viewer::render(frame, app, body_area, ss, ts, accent);
        frame.render_widget(render_hint_bar(app, tabs, accent), hint_area);
        return None;
    }

    if app.list_fullscreen {
        match app.mode {
            ViewMode::Status => file_list::render(frame, app, body_area, accent),
            ViewMode::Log => commit_list::render(frame, app, body_area, accent),
            ViewMode::Tree => tree_list::render(frame, app, body_area, accent),
        }
        frame.render_widget(render_hint_bar(app, tabs, accent), hint_area);
        return None;
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
    let cursor = terminal_tab::render(frame, app, main[1], accent);
    frame.render_widget(render_hint_bar(app, tabs, accent), hint_area);
    cursor
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
    let main = Layout::default()
        .direction(Direction::Vertical)
        .constraints(main_content_constraints(layout))
        .split(chrome_rows(screen_area).body);
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
    let body_area = chrome_rows(screen_area).body;

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
/// The notice row: a notice when one is raised, otherwise repo identity.
///
/// A notice covers the repo/branch line rather than taking a row of its own.
/// Adding a row would resize every PTY as notices come and go, and this is the
/// one chrome row whose content is ambient and re-derived every frame, so
/// covering it costs nothing — unlike the hint bar below, which holds the
/// repo-input text the user is editing.
fn render_notice_row<'a>(app: &'a App, accent: Color) -> Paragraph<'a> {
    if let Some(notice) = app.notice.as_ref() {
        return Paragraph::new(Line::from(Span::styled(
            format!(" {}", notice.line()),
            Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
        )));
    }
    render_repo_header(app, accent)
}

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

/// The `PREFIX` indicator chip. Shared by `render_hint_bar` and
/// `hint_click_at` so the click hit-test's column offset can never drift
/// from what is drawn.
const PREFIX_CHIP: &str = " PREFIX ";

/// The armed-prefix follow-up legend (everything after the chip). Single
/// source for rendering and click hit-testing.
fn prefix_armed_hint_text(app: &App) -> String {
    // While the terminal fills the body the digit row addresses panes
    // directly (`1-8`); in the split view `1`/`2` focus the list/diff and
    // `3-9,0` jump to panes (see `main::resolve_prefix_action`).
    let digits = if app.terminal.fullscreen.fills_body() {
        "1-8: pane"
    } else {
        "1-9: focus/pane"
    };
    // `w`/`s` only act under their availability predicates (see
    // `App::can_close_pane`/`can_swap_panes`), so only advertise them there —
    // a hint for a no-op key would lie.
    let close = if app.can_close_pane() {
        "w: close pane | "
    } else {
        ""
    };
    let swap = if app.can_swap_panes() {
        "s: swap pane | "
    } else {
        ""
    };
    // The view toggles name their destination from the current mode, matching
    // the normal legends, instead of a generic `log/status` label.
    let (log_toggle, tree_toggle) = match app.mode {
        ViewMode::Log => ("l: status view", "b: tree view"),
        ViewMode::Status => ("l: log view", "b: tree view"),
        ViewMode::Tree => ("l: log view", "b: status view"),
    };
    // `x` is advertised unconditionally, unlike `w`/`s` above: refusing to
    // close the last project reports why on the notice row, so the key always
    // produces a visible result rather than silently doing nothing.
    format!(
        " t: new pane | {close}{swap}{log_toggle} | {tree_toggle} | f: fullscreen | o: open project | x: close project | p: theme | r: redraw | q: quit | {digits} | esc: cancel"
    )
}

/// Build the styled spans for a hint legend, inverting (`REVERSED`) every
/// clickable segment — the whole `key: description` label, matching the
/// click target exactly — so the bar itself shows which hints respond to a
/// click. Consumes the same literal and `" | "` segmentation as
/// `hint_click_at` — and decides clickability with the same `segment_click`
/// — so an inverted label can never disagree with the hit test. Only styles
/// change; the rendered text (and thus every column offset) stays identical.
/// `mark_clickable` is `[mouse] enabled`: with capture off a click can never
/// arrive, so no label may advertise one.
fn hint_spans(text: &str, leader: &str, mark_clickable: bool) -> Vec<Span<'static>> {
    let base = Style::default().fg(Color::DarkGray);
    let inverted = base.add_modifier(Modifier::REVERSED);
    let mut spans = Vec::new();
    for (i, segment) in text.split(" | ").enumerate() {
        if i > 0 {
            spans.push(Span::styled(" | ", base));
        }
        let rendered = segment.replace("<prefix>", leader);
        let clickable = mark_clickable
            && segment
                .split_once(':')
                .and_then(|(keyspec, _)| segment_click(keyspec))
                .is_some();
        if clickable {
            // Invert the whole segment — the entire label is the click
            // target, so the affordance covers exactly what responds.
            // Leading whitespace stays plain so the chip doesn't start
            // with a stray block.
            let label_start = rendered.len() - rendered.trim_start().len();
            let (lead_ws, label) = rendered.split_at(label_start);
            if !lead_ws.is_empty() {
                spans.push(Span::styled(lead_ws.to_string(), base));
            }
            spans.push(Span::styled(label.to_string(), inverted));
        } else {
            spans.push(Span::styled(rendered, base));
        }
    }
    spans
}

fn render_hint_bar<'a>(app: &'a App, chrome: Chrome<'a>, accent: Color) -> Paragraph<'a> {
    if chrome.repo_input.active {
        // A rejected path is reported on the notice row directly above, so
        // this row stays a plain input line.
        return Paragraph::new(Line::from(vec![
            Span::styled("repo: ", Style::default().fg(accent)),
            Span::raw(chrome.repo_input.buf.as_str()),
            Span::styled("█", Style::default().fg(accent)),
        ]));
    }
    if app.prefix_armed() {
        let mut spans = vec![Span::styled(
            PREFIX_CHIP,
            Style::default()
                .fg(Color::Black)
                .bg(accent)
                .add_modifier(Modifier::BOLD),
        )];
        spans.extend(hint_spans(
            &prefix_armed_hint_text(app),
            &app.leader_label(),
            app.mouse_enabled,
        ));
        return Paragraph::new(Line::from(spans));
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
    // `<prefix>` in the hint literal resolves to the configured leader chord
    // (e.g. `^F`) so the footer always names the actual key to press rather
    // than an abstract word.
    Paragraph::new(Line::from(hint_spans(
        normal_hint_literal(app),
        &app.leader_label(),
        app.mouse_enabled,
    )))
}

/// The hint literal (with `<prefix>` placeholders) for the current
/// non-modal state. Single source for `render_hint_bar` and
/// `hint_click_at`, so the click hit-test always segments exactly the text
/// on screen.
fn normal_hint_literal(app: &App) -> &'static str {
    match app.terminal.fullscreen {
        // From Grid the next `f` zooms the active pane — but only when Zoom
        // would look different from Grid; otherwise the cycle skips Zoom and
        // `f` exits.
        TerminalFullscreen::Grid if app.terminal.zoom_distinct_from_grid() => {
            return " <prefix>: leader | shift+↑/↓: scroll | shift+pgup/dn: page scroll | shift+←/→: cycle pane | <prefix> f: zoom active pane | <prefix> t: new pane | <prefix> w: close pane | <prefix> q: quit";
        }
        TerminalFullscreen::Grid => {
            return " <prefix>: leader | shift+↑/↓: scroll | shift+pgup/dn: page scroll | <prefix> f: exit fullscreen | <prefix> t: new pane | <prefix> w: close pane | <prefix> q: quit";
        }
        TerminalFullscreen::Zoom => {
            return " <prefix>: leader | shift+↑/↓: scroll | shift+pgup/dn: page scroll | shift+←/→: cycle pane | <prefix> f: exit fullscreen | <prefix> t: new pane | <prefix> w: close pane | <prefix> q: quit";
        }
        TerminalFullscreen::Off => {}
    }
    if app.diff.fullscreen {
        let hint = if app.diff.view == DiffPaneView::File {
            // Tree mode's right pane is permanently the file view — `v`
            // can't leave it, so don't advertise a no-op.
            if app.mode == ViewMode::Tree {
                " <prefix> f: exit zoom | j/k: scroll | pgup/pgdn: page | <prefix> q: quit"
            } else {
                " <prefix> f: exit zoom | v: back to diff | j/k: scroll | pgup/pgdn: page | <prefix> q: quit"
            }
        } else if app.diff.view == DiffPaneView::Split {
            " <prefix> f: exit zoom | s: unified diff | j/k: scroll | pgup/pgdn: page | <prefix> q: quit"
        } else if app.diff.search.active {
            " type to search | enter: confirm | esc: cancel"
        } else if !app.diff.search.query.is_empty() {
            " <prefix> f: exit zoom | n: next match | shift+n: prev match | /: new search | esc: clear"
        } else if app.can_open_file_view() {
            " <prefix> f: exit zoom | j/k: scroll | v: view file | s: split | /: search | pgup/pgdn: page | <prefix> q: quit"
        } else {
            // No file target for `v` (log view browsing commits, or nothing
            // selected) — a hint for a no-op key would lie.
            " <prefix> f: exit zoom | j/k: scroll | s: split | /: search | pgup/pgdn: page | <prefix> q: quit"
        };
        return hint;
    }
    if app.list_fullscreen {
        let hint = match app.mode {
            ViewMode::Log if app.log_view.drill_down => {
                " <prefix> f: exit zoom | esc: back to commits | j/k: navigate files | <prefix> q: quit"
            }
            ViewMode::Log => {
                " <prefix> f: exit zoom | <prefix> l: status view | <prefix> b: tree view | j/k: navigate commits | enter: view files | <prefix> q: quit"
            }
            ViewMode::Status => {
                " <prefix> f: exit zoom | j/k: navigate | /: search | <prefix> l: log view | <prefix> b: tree view | <prefix> q: quit"
            }
            ViewMode::Tree => {
                " <prefix> f: exit zoom | j/k: navigate | /: search | →/enter: expand | ←: collapse | <prefix> b: status view | <prefix> l: log view | <prefix> q: quit"
            }
        };
        return hint;
    }
    if let Focus::Terminal = app.focus {
        // The `l` toggle names its destination: from Log mode it returns to
        // the status view, from Status/Tree it enters the log view.
        return if app.mode == ViewMode::Log {
            " <prefix>: leader | shift+↑/↓: scroll | shift+pgup/dn: page scroll | shift+←/→: cycle | <prefix> t: new pane | <prefix> w: close pane | <prefix> f: fullscreen | <prefix> l: status view | <prefix> o: open project | <prefix> q: quit"
        } else {
            " <prefix>: leader | shift+↑/↓: scroll | shift+pgup/dn: page scroll | shift+←/→: cycle | <prefix> t: new pane | <prefix> w: close pane | <prefix> f: fullscreen | <prefix> l: log view | <prefix> o: open project | <prefix> q: quit"
        };
    }
    match app.focus {
        Focus::Terminal => unreachable!("Focus::Terminal handled above"),
        Focus::FileList => match app.mode {
            ViewMode::Log => {
                if app.log_view.drill_down {
                    " esc: back to commits | j/k: navigate files | shift+←/→: cycle | <prefix> q: quit"
                } else {
                    " shift+←/→: cycle | j/k: navigate commits | enter: view files | <prefix> t: new pane | <prefix> f: fullscreen | <prefix> l: status view | <prefix> b: tree view | <prefix> o: open project | <prefix> q: quit"
                }
            }
            ViewMode::Status => {
                " shift+←/→: cycle | j/k: navigate | /: search | <prefix> t: new pane | <prefix> f: fullscreen | <prefix> l: log view | <prefix> b: tree view | <prefix> o: open project | <prefix> q: quit"
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
                // Tree mode's right pane is permanently the file view — `v`
                // can't leave it, so don't advertise a no-op.
                if app.mode == ViewMode::Tree {
                    " j/k: scroll | pgup/pgdn: page | /: search | shift+←/→: cycle | <prefix> q: quit"
                } else {
                    " v: back to diff | j/k: scroll | pgup/pgdn: page | /: search | shift+←/→: cycle | <prefix> q: quit"
                }
            } else if app.diff.view == DiffPaneView::Split {
                " s: unified diff | j/k: scroll | pgup/pgdn: page | shift+←/→: cycle | <prefix> f: zoom | <prefix> q: quit"
            } else if app.diff.search.active {
                " type to search | enter: confirm | esc: cancel"
            } else if !app.diff.search.query.is_empty() {
                " n: next match | shift+n: prev match | /: new search | esc: clear"
            } else if app.can_open_file_view() {
                // The `l` toggle names its destination (Tree mode never
                // reaches these arms — its right pane is always the file view).
                if app.mode == ViewMode::Log {
                    " shift+←/→: cycle | j/k: scroll | pgup/pgdn: scroll | v: view file | s: split | /: search | <prefix> t: new pane | <prefix> f: zoom | <prefix> l: status view | <prefix> b: tree view | <prefix> o: open project | <prefix> q: quit"
                } else {
                    " shift+←/→: cycle | j/k: scroll | pgup/pgdn: scroll | v: view file | s: split | /: search | <prefix> t: new pane | <prefix> f: zoom | <prefix> l: log view | <prefix> b: tree view | <prefix> o: open project | <prefix> q: quit"
                }
            } else {
                // No file target for `v` (log view browsing commits, or
                // nothing selected) — a hint for a no-op key would lie.
                if app.mode == ViewMode::Log {
                    " shift+←/→: cycle | j/k: scroll | pgup/pgdn: scroll | s: split | /: search | <prefix> t: new pane | <prefix> f: zoom | <prefix> l: status view | <prefix> b: tree view | <prefix> o: open project | <prefix> q: quit"
                } else {
                    " shift+←/→: cycle | j/k: scroll | pgup/pgdn: scroll | s: split | /: search | <prefix> t: new pane | <prefix> f: zoom | <prefix> l: log view | <prefix> b: tree view | <prefix> o: open project | <prefix> q: quit"
                }
            }
        }
    }
}

/// The pane index a click at screen cell `(x, y)` on the terminal tab bar
/// jumps to — a tab targets its own pane, a `+N` hidden marker the nearest
/// hidden pane on its side. `None` off the tab row or when the terminal
/// isn't drawn (another panel fullscreen). Mirrors `pane_at`'s geometry
/// sourcing: the tab row comes from the same `terminal_widget_area` the
/// renderer draws into.
pub(crate) fn tab_click_at(
    app: &App,
    screen_area: Rect,
    layout: &LayoutConfig,
    x: u16,
    y: u16,
) -> Option<usize> {
    let widget_area = terminal_widget_area(app, screen_area, layout)?;
    terminal_tab::tab_target_at(app, widget_area, x, y)
}

/// A clickable hint-bar shortcut, expressed as the key(s) the label names so
/// the caller can dispatch it through the exact same path as a keypress.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum HintClick {
    /// The bare `<prefix>` label — press the leader chord alone, arming the
    /// prefix so the armed follow-up row (itself clickable) takes over.
    Arm,
    /// `<prefix> c` — press the leader chord, then `c`.
    Leader(char),
    /// A bare key `c` (a focus-local command, or an armed-prefix follow-up —
    /// the armed state already lives in `App`, so the bare key resolves).
    Plain(char),
}

/// The clickable shortcut under screen cell `(x, y)` on the bottom hint row,
/// or `None` for anything else: a cell off the hint row, an informational
/// segment (`j/k: navigate`, digit legends), a separator, or a modal row
/// (repo input, swap target, status message — none carry clickable hints).
///
/// Segments the same text `render_hint_bar` draws — `prefix_armed_hint_text`
/// / `normal_hint_literal` are shared — measuring rendered display widths,
/// so the hit test cannot drift from the screen.
pub(crate) fn hint_click_at(
    app: &App,
    chrome: Chrome<'_>,
    screen_area: Rect,
    x: u16,
    y: u16,
) -> Option<HintClick> {
    // With mouse capture off no click can reach us anyway, but the bar also
    // renders no inverted labels (`hint_spans`) — keep the affordance and the
    // hit test in agreement rather than relying on the caller.
    if !app.mouse_enabled {
        return None;
    }
    let hint_area = chrome_rows(screen_area).hint;
    if hint_area.height == 0 || !hint_area.contains(Position { x, y }) {
        return None;
    }

    // Row selection mirrors `render_hint_bar`'s branch order exactly, or a
    // click would resolve against a row the user isn't looking at. Notices no
    // longer appear here (they own the row above), so they don't feature.
    let (chip, text) = if chrome.repo_input.active {
        return None;
    } else if app.prefix_armed() {
        (PREFIX_CHIP, prefix_armed_hint_text(app))
    } else if app.awaiting_swap_target() {
        return None;
    } else {
        ("", normal_hint_literal(app).to_string())
    };

    let leader = app.leader_label();
    let mut cursor = hint_area.x + Span::raw(chip).width() as u16;
    for (i, segment) in text.split(" | ").enumerate() {
        if i > 0 {
            cursor += Span::raw(" | ").width() as u16;
        }
        let rendered = segment.replace("<prefix>", &leader);
        let width = Span::raw(rendered.as_str()).width() as u16;
        if x >= cursor && x < cursor + width {
            // Leading whitespace renders plain (see `hint_spans`), so it is
            // not part of the click target either — the clickable range must
            // match the inverted label cell for cell.
            let label_start = rendered.len() - rendered.trim_start().len();
            let lead_width = Span::raw(&rendered[..label_start]).width() as u16;
            if x < cursor + lead_width {
                return None;
            }
            let (keyspec, _) = segment.split_once(':')?;
            return segment_click(keyspec);
        }
        cursor += width;
    }
    None
}

/// Map a hint segment's key label to its click action. The bare `<prefix>`
/// label arms the prefix (the armed row's follow-ups are clickable in turn,
/// completing a mouse-only flow). Beyond that, only discrete commands are
/// clickable; everything else returns `None`:
/// - continuous navigation (`j/k`, `shift+↑/↓`, `pgup/pgdn`, …) — a click
///   has no sensible single-step meaning,
/// - digit legends (`1-8`, `3-9,0`) — the digit is the argument, a click
///   doesn't name one,
/// - `q: quit` — deliberately excluded so one stray click can't end the
///   session,
/// - `esc`/`enter` and free-text labels.
fn segment_click(keyspec: &str) -> Option<HintClick> {
    let spec = keyspec.trim();
    if spec == "<prefix>" {
        return Some(HintClick::Arm);
    }
    if let Some(rest) = spec.strip_prefix("<prefix> ") {
        let mut chars = rest.chars();
        if let (Some(c), None) = (chars.next(), chars.next())
            && matches!(c, 't' | 'w' | 'f' | 'l' | 'b' | 'o')
        {
            return Some(HintClick::Leader(c));
        }
        return None;
    }
    let mut chars = spec.chars();
    if let (Some(c), None) = (chars.next(), chars.next())
        && matches!(
            c,
            't' | 'w' | 's' | 'l' | 'b' | 'f' | 'o' | 'x' | 'p' | 'r' | 'v' | '/'
        )
    {
        return Some(HintClick::Plain(c));
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::NoticeKind;
    use crate::app::tests::{app_with_fake_backend, app_with_files};
    use crate::runtime::terminal::TerminalFullscreen;
    use ratatui::{Terminal, backend::TestBackend};

    /// Render the notice row into a wide buffer and return its flattened text.
    fn notice_text(app: &App) -> String {
        let mut terminal = Terminal::new(TestBackend::new(200, 1)).unwrap();
        terminal
            .draw(|frame| frame.render_widget(render_notice_row(app, Color::Yellow), frame.area()))
            .unwrap();
        let buf = terminal.backend().buffer();
        (0..buf.area.width)
            .map(|x| buf[(x, 0)].symbol())
            .collect::<String>()
    }

    /// Render the hint bar into a wide buffer and return its flattened text so
    /// footer wording can be asserted layout by layout.
    /// A workspace holding one project, for the dialog tests — the dialog is
    /// workspace state, but its rejection notice lands on the active project.
    fn test_workspace() -> crate::workspace::Workspace {
        let mut ws = crate::workspace::Workspace::new(crossterm::event::KeyEvent::new(
            crossterm::event::KeyCode::Char('f'),
            crossterm::event::KeyModifiers::CONTROL,
        ));
        ws.add(app_with_files(vec![]));
        ws
    }

    /// A chrome view with no tabs and a closed dialog — the shape most hint
    /// assertions want, since they are about one project's footer.
    fn plain_chrome(repo_input: &RepoInput) -> Chrome<'_> {
        Chrome {
            repo_paths: &[],
            active: 0,
            repo_input,
        }
    }

    fn hint_text(app: &App) -> String {
        let repo_input = RepoInput::default();
        hint_text_with(app, plain_chrome(&repo_input))
    }

    fn hint_text_with(app: &App, chrome: Chrome<'_>) -> String {
        let mut terminal = Terminal::new(TestBackend::new(200, 1)).unwrap();
        terminal
            .draw(|frame| {
                frame.render_widget(render_hint_bar(app, chrome, Color::Yellow), frame.area())
            })
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

    /// The inversion is the clickability affordance, so the two must agree
    /// cell for cell: every column the hint bar renders REVERSED must
    /// resolve to a click action, every clickable column must render
    /// REVERSED, and at least one such column must exist.
    fn assert_inverted_cells_are_clickable(app: &App) {
        let repo_input = RepoInput::default();
        let chrome = plain_chrome(&repo_input);
        let mut terminal = Terminal::new(TestBackend::new(200, 1)).unwrap();
        terminal
            .draw(|frame| {
                frame.render_widget(render_hint_bar(app, chrome, Color::Yellow), frame.area())
            })
            .unwrap();
        let buf = terminal.backend().buffer();
        // `hint_click_at` takes full-screen coordinates: hint row = row 2 of
        // a 3-row screen, with the same x origin as the 1-row render above.
        let screen = Rect::new(0, 0, 200, 3);
        let mut inverted = 0;
        for x in 0..200u16 {
            let is_inverted = buf[(x, 0)].modifier.contains(Modifier::REVERSED);
            let is_clickable = hint_click_at(app, chrome, screen, x, 2).is_some();
            assert_eq!(
                is_inverted, is_clickable,
                "hint cell at column {x}: inverted={is_inverted} but clickable={is_clickable}"
            );
            inverted += is_inverted as u32;
        }
        assert!(
            inverted > 0,
            "at least one clickable key label must render inverted"
        );
    }

    /// A rejected path is reported on the notice row while the input keeps the
    /// text being corrected on the row below. The message used to be written to
    /// a field the repo-input row never rendered, so the confirm looked like it
    /// did nothing at all.
    #[test]
    fn repo_input_reports_a_rejected_path_on_the_notice_row() {
        let mut ws = test_workspace();
        ws.start_repo_input();
        ws.repo_input.buf = "/definitely/not/here".to_string();
        ws.confirm_repo_input();

        assert!(
            ws.repo_input.active,
            "a rejected path must leave the dialog open for correction"
        );
        // With a project open the rejection lands on that project's notice row,
        // directly above the input still holding the text to correct.
        let notice = notice_text(ws.active().unwrap());
        assert!(
            notice.contains("no such directory"),
            "the notice row must say why the confirm was rejected, got: {notice}"
        );
        let repo_input = ws.repo_input.clone();
        let hint = hint_text_with(ws.active().unwrap(), plain_chrome(&repo_input));
        assert!(
            hint.contains("/definitely/not/here"),
            "the rejected text must stay in the input, got: {hint}"
        );
    }

    #[test]
    fn repo_input_notice_clears_once_the_path_is_edited() {
        let mut ws = test_workspace();
        ws.start_repo_input();
        ws.repo_input.buf = "/definitely/not/here".to_string();
        ws.confirm_repo_input();
        ws.repo_input_pop();

        let notice = notice_text(ws.active().unwrap());
        assert!(
            !notice.contains("no such directory"),
            "editing the path must clear the stale verdict, got: {notice}"
        );
    }

    /// The notice row is the one place every kind reports, and no overlay may
    /// shadow it — the hint bar's own early-returns are what made a notice
    /// invisible before it moved off that row.
    #[test]
    fn notice_row_shows_notices_through_every_overlay() {
        for setup in [
            (|app: &mut App| app.arm_prefix()) as fn(&mut App),
            |app: &mut App| app.begin_swap_target(),
        ] {
            let mut app = app_with_fake_backend();
            setup(&mut app);
            app.raise_notice(NoticeKind::Git, "not a repo");
            let text = notice_text(&app);
            assert!(
                text.contains("git error: not a repo"),
                "an open overlay must not shadow the notice row, got: {text}"
            );
        }
    }

    /// With nothing raised the row is the repo/branch line, and it comes back
    /// intact after a notice is cleared.
    #[test]
    fn notice_row_falls_back_to_repo_identity() {
        let mut app = app_with_files(vec![]);
        app.repo_path = "/tmp/somewhere".to_string();
        let before = notice_text(&app);
        assert!(before.contains("/tmp/somewhere"), "got: {before}");

        app.raise_notice(NoticeKind::Tree, "boom");
        assert!(!notice_text(&app).contains("/tmp/somewhere"));

        app.clear_notice(NoticeKind::Tree);
        assert_eq!(notice_text(&app), before);
    }

    #[test]
    fn hint_bar_inverts_only_clickable_key_labels() {
        let app = app_with_fake_backend();
        assert_inverted_cells_are_clickable(&app);
    }

    /// Render a full frame and flatten it to text.
    fn drawn_text(app: &mut App, tab_paths: &[String], active: usize) -> String {
        let mut terminal = Terminal::new(TestBackend::new(120, 20)).unwrap();
        let ss = two_face::syntax::extra_newlines();
        let ts = ThemeSet::load_defaults();
        terminal
            .draw(|frame| {
                let tabs = Chrome {
                    repo_paths: tab_paths,
                    active,
                    repo_input: &RepoInput::default(),
                };
                draw(
                    frame,
                    app,
                    tabs,
                    &ss,
                    &ts,
                    &LayoutConfig::default(),
                    Color::Yellow,
                );
            })
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

    /// Render the empty screen and flatten it to text.
    fn drawn_empty(
        repo_input: &RepoInput,
        notice: Option<&crate::app::Notice>,
        armed: bool,
    ) -> String {
        let mut terminal = Terminal::new(TestBackend::new(90, 12)).unwrap();
        let leader = crossterm::event::KeyEvent::new(
            crossterm::event::KeyCode::Char('f'),
            crossterm::event::KeyModifiers::CONTROL,
        );
        terminal
            .draw(|frame| {
                let chrome = Chrome {
                    repo_paths: &[],
                    active: 0,
                    repo_input,
                };
                draw_empty(frame, chrome, notice, leader, armed, false, Color::Yellow);
            })
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
    fn the_empty_screen_names_the_only_two_things_that_work() {
        let text = drawn_empty(&RepoInput::default(), None, false);

        assert!(text.contains("no project open"), "got: {text}");
        assert!(text.contains("^F o: open project"), "got: {text}");
        assert!(text.contains("^F q: quit"), "got: {text}");
    }

    #[test]
    fn the_empty_screen_shows_the_prefix_chip_when_armed() {
        // Pressing the leader with no project open has to look like it did
        // something, or it reads as a dead key.
        let text = drawn_empty(&RepoInput::default(), None, true);

        assert!(text.contains("PREFIX"), "got: {text}");
        assert!(text.contains("o: open project"), "got: {text}");
        assert!(text.contains("esc: cancel"), "got: {text}");
    }

    #[test]
    fn the_empty_screen_shows_the_dialog_and_its_rejection() {
        // The dialog and its notice are the reason the empty screen keeps its
        // chrome at all — a rejected path must still report why.
        let repo_input = RepoInput {
            active: true,
            buf: "/definitely/not/here".to_string(),
            prefilled: false,
        };
        let notice = crate::app::Notice::new(NoticeKind::RepoInput, "no such directory");

        let text = drawn_empty(&repo_input, Some(&notice), false);

        assert!(text.contains("repo: /definitely/not/here"), "got: {text}");
        assert!(text.contains("no such directory"), "got: {text}");
    }

    #[test]
    fn the_project_tab_row_survives_every_fullscreen_mode() {
        // Chrome is rendered before the layout branches precisely so no view
        // mode can strand the user without knowing which project they are in.
        let paths = vec!["/w/api".to_string(), "/w/web".to_string()];

        let mut app = app_with_fake_backend();
        assert!(drawn_text(&mut app, &paths, 0).contains("F2 web"), "split");

        let mut app = app_with_fake_backend();
        app.terminal.fullscreen = TerminalFullscreen::Grid;
        assert!(
            drawn_text(&mut app, &paths, 0).contains("F2 web"),
            "terminal fullscreen"
        );

        let mut app = app_with_files(vec!["a.rs"]);
        app.list_fullscreen = true;
        assert!(
            drawn_text(&mut app, &paths, 0).contains("F2 web"),
            "list fullscreen"
        );

        let mut app = app_with_files(vec!["a.rs"]);
        app.diff.fullscreen = true;
        assert!(
            drawn_text(&mut app, &paths, 0).contains("F2 web"),
            "diff fullscreen"
        );
    }

    #[test]
    fn project_tab_at_matches_the_rendered_row() {
        // The hit test derives from `chrome_rows` like `draw` does, so a click
        // on a tab's glyphs must resolve to that tab.
        let mut app = app_with_files(vec!["a.rs"]);
        let paths = vec!["/w/api".to_string(), "/w/web".to_string()];
        let screen = Rect::new(0, 0, 120, 20);
        let text = drawn_text(&mut app, &paths, 0);
        let first_row = text.lines().next().unwrap();
        let web_x = first_row.find("F2 web").expect("second tab rendered") as u16;

        let tabs = Chrome {
            repo_paths: &paths,
            active: 0,
            repo_input: &RepoInput::default(),
        };
        assert_eq!(project_tab_at(tabs, screen, 0, 0), Some(0));
        assert_eq!(project_tab_at(tabs, screen, web_x, 0), Some(1));
        // Row 1 is the body, not the tab row.
        assert_eq!(project_tab_at(tabs, screen, web_x, 1), None);
    }

    #[test]
    fn panels_advertise_the_leader_digit_not_the_bare_f_key() {
        // The bare F-key row selects project tabs, so a panel legend reading
        // `F1 Files` would name a key that switches projects instead of
        // focusing the panel.
        let mut app = app_with_files(vec!["a.rs"]);
        let tab_paths = vec![".".to_string()];
        let mut terminal = Terminal::new(TestBackend::new(120, 20)).unwrap();
        let ss = two_face::syntax::extra_newlines();
        let ts = ThemeSet::load_defaults();
        terminal
            .draw(|frame| {
                draw(
                    frame,
                    &mut app,
                    Chrome {
                        repo_paths: &tab_paths,
                        active: 0,
                        repo_input: &RepoInput::default(),
                    },
                    &ss,
                    &ts,
                    &LayoutConfig::default(),
                    Color::Yellow,
                );
            })
            .unwrap();

        let buf = terminal.backend().buffer();
        let text: String = (0..buf.area.height)
            .map(|y| {
                (0..buf.area.width)
                    .map(|x| buf[(x, y)].symbol())
                    .collect::<String>()
            })
            .collect::<Vec<_>>()
            .join("\n");

        assert!(
            text.contains("^F1 Files"),
            "file list must advertise its leader digit, got: {text}"
        );
        // The `Ctrl+F` leader label ("^F") ends in the letter F, so the legit
        // "^F1 Files" legend contains "F1 Files" as a substring. Strip it before
        // asserting the bare function-key legend never appears on its own.
        assert!(
            !text.replace("^F1 Files", "").contains("F1 Files"),
            "the bare F-key must not be advertised for panels, got: {text}"
        );
    }

    #[test]
    fn no_hint_row_advertises_the_removed_change_repo_command() {
        // `o` opened the change-this-tab's-repo dialog before tabs existed.
        // Two terminal-focus rows kept saying "repo" long after it started
        // opening a tab instead — a rename that no test was watching.
        let mut app = app_with_fake_backend();
        for mode in [ViewMode::Status, ViewMode::Log, ViewMode::Tree] {
            for focus in [Focus::Terminal, Focus::FileList, Focus::DiffViewer] {
                app.mode = mode;
                app.focus = focus;
                let text = normal_hint_literal(&app);
                assert!(
                    !text.contains("o: repo"),
                    "{mode:?}/{focus:?} still advertises the removed command: {text}"
                );
            }
        }
    }

    #[test]
    fn armed_prefix_hint_advertises_the_project_keys() {
        let mut app = app_with_fake_backend();
        app.arm_prefix();

        let text = hint_text(&app);

        assert!(text.contains("o: open project"), "got: {text}");
        assert!(text.contains("x: close project"), "got: {text}");
    }

    /// The terminal-focus legend is the one carrying the bare
    /// `<prefix>: leader` segment — its inversion must round-trip to a
    /// click like every other clickable label.
    #[test]
    fn terminal_focus_hint_bar_inverts_only_clickable_key_labels() {
        let mut app = app_with_fake_backend();
        app.focus = Focus::Terminal;
        assert_inverted_cells_are_clickable(&app);
    }

    #[test]
    fn bare_prefix_segment_resolves_to_an_arm_click() {
        assert_eq!(segment_click("<prefix>"), Some(HintClick::Arm));
        assert_eq!(segment_click(" <prefix>"), Some(HintClick::Arm));
    }

    #[test]
    fn armed_prefix_hint_bar_inverts_only_clickable_key_labels() {
        let mut app = app_with_fake_backend();
        app.arm_prefix();
        assert_inverted_cells_are_clickable(&app);
    }

    /// Notices own the row above the hint bar, so raising one must not
    /// disturb the hint bar's labels or their click targets.
    #[test]
    fn armed_prefix_hint_bar_stays_clickable_while_a_notice_is_set() {
        let mut app = app_with_fake_backend();
        app.arm_prefix();
        app.raise_notice(NoticeKind::Git, "boom");
        assert_inverted_cells_are_clickable(&app);
    }

    #[test]
    fn hint_bar_inverts_nothing_when_mouse_capture_is_disabled() {
        let mut app = app_with_fake_backend();
        app.mouse_enabled = false;
        let mut terminal = Terminal::new(TestBackend::new(200, 1)).unwrap();
        terminal
            .draw(|frame| {
                frame.render_widget(
                    render_hint_bar(&app, plain_chrome(&RepoInput::default()), Color::Yellow),
                    frame.area(),
                )
            })
            .unwrap();
        let buf = terminal.backend().buffer();

        let inverted = (0..200u16).any(|x| buf[(x, 0)].modifier.contains(Modifier::REVERSED));

        assert!(
            !inverted,
            "with the mouse handed back to the terminal, no hint may \
             advertise a click that cannot arrive"
        );
    }

    /// The affordance/hit-test agreement holds with the mouse disabled too:
    /// nothing renders inverted, so nothing may resolve to a click.
    #[test]
    fn hint_click_resolves_nothing_when_mouse_capture_is_disabled() {
        let mut app = app_with_fake_backend();
        app.focus = Focus::Terminal;
        app.mouse_enabled = false;
        let screen = Rect::new(0, 0, 200, 3);
        for x in 0..200u16 {
            assert_eq!(
                hint_click_at(&app, plain_chrome(&RepoInput::default()), screen, x, 2),
                None,
                "x={x} resolves to a click the disabled mouse can never send"
            );
        }
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

    /// `<leader> w` only closes with terminal focus (`handle_global_action`
    /// scopes it), so both the armed row and the normal legends must only
    /// advertise it there — a hint for a no-op key would lie.
    #[test]
    fn prefix_hint_advertises_close_only_with_terminal_focus() {
        let mut upper = app_with_fake_backend();
        upper.arm_prefix();
        assert!(
            !hint_text(&upper).contains("w: close"),
            "armed row must not offer close without terminal focus"
        );

        let mut term = app_with_fake_backend();
        term.focus = Focus::Terminal;
        term.arm_prefix();
        assert!(
            hint_text(&term).contains("w: close"),
            "armed row must offer close with terminal focus"
        );
    }

    /// The armed row's `w: close` must round-trip to a click exactly when it
    /// is shown: some column resolves to `Plain('w')` with terminal focus,
    /// and no column does without it (the segment isn't rendered, so a click
    /// target for it would be a phantom).
    #[test]
    fn armed_prefix_close_click_target_follows_terminal_focus() {
        let screen = Rect::new(0, 0, 200, 3);
        let clicks = |app: &App| {
            (0..200u16)
                .filter(|&x| {
                    hint_click_at(app, plain_chrome(&RepoInput::default()), screen, x, 2)
                        == Some(HintClick::Plain('w'))
                })
                .count()
        };

        let mut term = app_with_fake_backend();
        term.focus = Focus::Terminal;
        term.arm_prefix();
        assert!(
            clicks(&term) > 0,
            "terminal-focused armed row must offer a close click target"
        );

        let mut upper = app_with_fake_backend();
        upper.arm_prefix();
        assert_eq!(
            clicks(&upper),
            0,
            "non-terminal armed row must not resolve any cell to a close click"
        );
    }

    /// `<leader> s` shares close's scoping (`handle_global_action`): terminal
    /// focus plus a second pane to swap with. The armed row must only
    /// advertise it then — a hint for a no-op key would lie.
    #[test]
    fn prefix_hint_advertises_swap_only_when_a_swap_can_act() {
        let mut upper = app_with_fake_backend();
        upper.terminal.create_pane().unwrap();
        upper.terminal.create_pane().unwrap();
        upper.focus = Focus::FileList;
        upper.arm_prefix();
        assert!(
            !hint_text(&upper).contains("s: swap pane"),
            "armed row must not offer swap without terminal focus"
        );

        let mut single = app_with_fake_backend();
        single.terminal.create_pane().unwrap();
        single.focus = Focus::Terminal;
        single.arm_prefix();
        assert!(
            !hint_text(&single).contains("s: swap pane"),
            "armed row must not offer swap with a single pane"
        );

        let mut term = app_with_fake_backend();
        term.terminal.create_pane().unwrap();
        term.terminal.create_pane().unwrap();
        term.focus = Focus::Terminal;
        term.arm_prefix();
        assert!(
            hint_text(&term).contains("s: swap pane"),
            "armed row must offer swap with terminal focus and two panes"
        );
    }

    /// The armed row's view toggles name their destination from the current
    /// mode, mirroring the normal legends' `l: log view`/`l: status view`
    /// wording instead of a generic `log/status` label.
    #[test]
    fn prefix_hint_names_view_toggle_destinations_by_mode() {
        let mut app = app_with_fake_backend();
        app.arm_prefix();

        let text = hint_text(&app);
        assert!(
            text.contains("l: log view") && text.contains("b: tree view"),
            "status mode armed row must name log/tree destinations, got: {text}"
        );

        app.mode = ViewMode::Log;
        let text = hint_text(&app);
        assert!(
            text.contains("l: status view") && text.contains("b: tree view"),
            "log mode armed row must name status/tree destinations, got: {text}"
        );

        app.mode = ViewMode::Tree;
        let text = hint_text(&app);
        assert!(
            text.contains("l: log view") && text.contains("b: status view"),
            "tree mode armed row must name log/status destinations, got: {text}"
        );
    }

    /// Every upper legend advertises both view toggles with destination
    /// labels — `l` (log ↔ status) and `b` (tree ↔ status) act from any
    /// focus, so no mode may hide one or name the view already shown.
    #[test]
    fn upper_legends_advertise_both_view_toggles() {
        // FileList browsing commits in Log mode.
        let mut app = app_with_fake_backend();
        app.mode = ViewMode::Log;
        let text = hint_text(&app);
        assert!(
            text.contains("l: status view") && text.contains("b: tree view"),
            "log list legend must offer both toggles, got: {text}"
        );

        // DiffViewer in Log mode: `l` names status, not the view shown.
        app.focus = Focus::DiffViewer;
        let text = hint_text(&app);
        assert!(
            text.contains("l: status view") && text.contains("b: tree view"),
            "log diff legend must offer both toggles, got: {text}"
        );

        // Terminal focus in Log mode follows the same destination wording.
        app.focus = Focus::Terminal;
        let text = hint_text(&app);
        assert!(
            text.contains("l: status view"),
            "log terminal legend must name the status destination, got: {text}"
        );

        // Zoomed list rows carry both toggles in every mode.
        let mut zoomed = app_with_fake_backend();
        zoomed.list_fullscreen = true;
        let text = hint_text(&zoomed);
        assert!(
            text.contains("l: log view") && text.contains("b: tree view"),
            "zoomed status list must offer both toggles, got: {text}"
        );
        zoomed.mode = ViewMode::Log;
        let text = hint_text(&zoomed);
        assert!(
            text.contains("l: status view") && text.contains("b: tree view"),
            "zoomed log list must offer both toggles, got: {text}"
        );
        zoomed.mode = ViewMode::Tree;
        let text = hint_text(&zoomed);
        assert!(
            text.contains("b: status view") && text.contains("l: log view"),
            "zoomed tree list must offer both toggles, got: {text}"
        );
    }

    #[test]
    fn normal_hint_advertises_close_only_with_terminal_focus() {
        let mut app = app_with_fake_backend();
        for focus in [Focus::FileList, Focus::DiffViewer] {
            app.focus = focus;
            let text = hint_text(&app);
            assert!(
                !text.contains("w: close pane"),
                "{focus:?} legend must not offer close, got: {text}"
            );
        }
        app.focus = Focus::Terminal;
        assert!(
            hint_text(&app).contains("w: close pane"),
            "terminal legend must offer close"
        );
    }

    /// `v` only opens a file when `current_file_view_key` resolves (log view
    /// needs a drill-down file selection), so the diff legend must only
    /// advertise `v: view file` then — a hint for a no-op key would lie.
    #[test]
    fn diff_hint_advertises_view_file_only_with_a_file_target() {
        // Log view browsing commits (no drill-down): `v` has no target.
        let mut app = app_with_fake_backend();
        app.mode = ViewMode::Log;
        app.focus = Focus::DiffViewer;
        let text = hint_text(&app);
        assert!(
            !text.contains("v: view file"),
            "commit-level log legend must not offer view file, got: {text}"
        );
        assert!(
            text.contains("s: split"),
            "split still acts on the commit diff, got: {text}"
        );

        // Same state zoomed: the fullscreen legend must agree.
        app.diff.fullscreen = true;
        let text = hint_text(&app);
        assert!(
            !text.contains("v: view file"),
            "zoomed commit-level legend must not offer view file, got: {text}"
        );

        // Drill-down with a file selected: `v` acts, so advertise it.
        app.diff.fullscreen = false;
        app.log_view
            .set_commits(vec![crate::git::diff::CommitEntry::new(
                git2::Oid::ZERO_SHA1,
                "deadbee".to_string(),
                "c".to_string(),
                "T".to_string(),
                0,
            )]);
        app.log_view.drill_down = true;
        app.log_view.commit_files = vec![crate::git::diff::ChangedFile::unstaged_only(
            "a.rs".to_string(),
            StatusKind::Modified,
        )];
        assert!(
            hint_text(&app).contains("v: view file"),
            "drill-down legend must offer view file"
        );

        // Status view with a selected file (the fixture's default list).
        let mut status = app_with_fake_backend();
        status.focus = Focus::DiffViewer;
        assert!(
            hint_text(&status).contains("v: view file"),
            "status legend must offer view file for a selected file"
        );
    }

    /// Tree mode's right pane is permanently the file view — `v` never
    /// toggles there, so the file-view legend must not offer `back to diff`.
    #[test]
    fn tree_file_view_hint_omits_back_to_diff() {
        let mut app = app_with_fake_backend();
        app.mode = ViewMode::Tree;
        app.focus = Focus::DiffViewer;
        app.diff.view = DiffPaneView::File;
        let text = hint_text(&app);
        assert!(
            !text.contains("v: back to diff"),
            "tree file-view legend must not offer back to diff, got: {text}"
        );

        app.diff.fullscreen = true;
        let text = hint_text(&app);
        assert!(
            !text.contains("v: back to diff"),
            "zoomed tree file-view legend must not offer back to diff, got: {text}"
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

        // Full screen keeps all three chrome rows (project tabs on top,
        // notice + hint below), then the terminal widget consumes one pane tab
        // row and the top/bottom border rows. Side borders were dropped, so the
        // content spans the full width. A single pane has no per-cell border,
        // so its content Rect equals the whole terminal content area.
        assert_eq!(areas.len(), 1);
        assert_eq!(areas[0].0, 1);
        assert_eq!(areas[0].1.height, 34);
        assert_eq!(areas[0].1.width, 100);
    }

    /// x column where `needle` starts on the rendered hint row, measured in
    /// display cells over exactly the text the renderer draws.
    fn hint_x_of(app: &App, needle: &str) -> u16 {
        let (chip, text) = if app.prefix_armed() {
            (PREFIX_CHIP, prefix_armed_hint_text(app))
        } else {
            (
                "",
                normal_hint_literal(app).replace("<prefix>", &app.leader_label()),
            )
        };
        let full = format!("{chip}{text}");
        let byte = full.find(needle).expect("needle must be on the hint row");
        Span::raw(&full[..byte]).width() as u16
    }

    const HINT_TEST_SCREEN: Rect = Rect::new(0, 0, 300, 40);
    const HINT_ROW: u16 = 39;

    #[test]
    fn hint_click_resolves_commands_and_skips_nav_and_quit() {
        // Default state: FileList focus, status view — the row carries both
        // leader commands and nav segments.
        let app = app_with_fake_backend();

        let x = hint_x_of(&app, "t: new pane");
        assert_eq!(
            hint_click_at(
                &app,
                plain_chrome(&RepoInput::default()),
                HINT_TEST_SCREEN,
                x,
                HINT_ROW
            ),
            Some(HintClick::Leader('t'))
        );
        let x = hint_x_of(&app, "/: search");
        assert_eq!(
            hint_click_at(
                &app,
                plain_chrome(&RepoInput::default()),
                HINT_TEST_SCREEN,
                x,
                HINT_ROW
            ),
            Some(HintClick::Plain('/'))
        );
        let x = hint_x_of(&app, "j/k: navigate");
        assert_eq!(
            hint_click_at(
                &app,
                plain_chrome(&RepoInput::default()),
                HINT_TEST_SCREEN,
                x,
                HINT_ROW
            ),
            None
        );
        let x = hint_x_of(&app, "q: quit");
        assert_eq!(
            hint_click_at(
                &app,
                plain_chrome(&RepoInput::default()),
                HINT_TEST_SCREEN,
                x,
                HINT_ROW
            ),
            None,
            "quit must never be one stray click away"
        );
    }

    #[test]
    fn hint_click_agrees_with_the_rendered_buffer_not_just_the_builder() {
        // Independent cross-check: locate the label in the *rendered* buffer
        // (no shared width math with `hint_click_at`) and hit-test there. If
        // renderer and hit test ever segment differently, this drifts.
        let app = app_with_fake_backend();
        let mut terminal = Terminal::new(TestBackend::new(300, 1)).unwrap();
        terminal
            .draw(|frame| {
                frame.render_widget(
                    render_hint_bar(&app, plain_chrome(&RepoInput::default()), Color::Yellow),
                    frame.area(),
                )
            })
            .unwrap();
        let buf = terminal.backend().buffer();
        // Scan cell-wise so the needle's index is a *column*, not a byte
        // offset — the row contains multi-byte arrows before the label.
        let cells: Vec<&str> = (0..buf.area.width).map(|x| buf[(x, 0)].symbol()).collect();
        let x = (0..cells.len())
            .find(|&i| cells[i..].concat().starts_with("t: new pane"))
            .expect("label rendered") as u16;

        assert_eq!(
            hint_click_at(
                &app,
                plain_chrome(&RepoInput::default()),
                HINT_TEST_SCREEN,
                x,
                HINT_ROW
            ),
            Some(HintClick::Leader('t'))
        );
    }

    #[test]
    fn hint_click_misses_off_the_hint_row() {
        let app = app_with_fake_backend();
        let x = hint_x_of(&app, "t: new pane");
        assert_eq!(
            hint_click_at(
                &app,
                plain_chrome(&RepoInput::default()),
                HINT_TEST_SCREEN,
                x,
                HINT_ROW - 1
            ),
            None
        );
    }

    #[test]
    fn hint_click_armed_row_resolves_bare_followups_after_the_chip() {
        let mut app = app_with_fake_backend();
        app.arm_prefix();

        let x = hint_x_of(&app, "t: new pane");
        assert_eq!(
            hint_click_at(
                &app,
                plain_chrome(&RepoInput::default()),
                HINT_TEST_SCREEN,
                x,
                HINT_ROW
            ),
            Some(HintClick::Plain('t'))
        );
        let x = hint_x_of(&app, "r: redraw");
        assert_eq!(
            hint_click_at(
                &app,
                plain_chrome(&RepoInput::default()),
                HINT_TEST_SCREEN,
                x,
                HINT_ROW
            ),
            Some(HintClick::Plain('r'))
        );
        let x = hint_x_of(&app, "q: quit");
        assert_eq!(
            hint_click_at(
                &app,
                plain_chrome(&RepoInput::default()),
                HINT_TEST_SCREEN,
                x,
                HINT_ROW
            ),
            None
        );
        let x = hint_x_of(&app, "esc: cancel");
        assert_eq!(
            hint_click_at(
                &app,
                plain_chrome(&RepoInput::default()),
                HINT_TEST_SCREEN,
                x,
                HINT_ROW
            ),
            None
        );
    }

    #[test]
    fn hint_click_none_on_modal_rows() {
        let mut swap = app_with_fake_backend();
        swap.begin_swap_target();
        assert!((0..HINT_TEST_SCREEN.width).all(|x| {
            hint_click_at(
                &swap,
                plain_chrome(&RepoInput::default()),
                HINT_TEST_SCREEN,
                x,
                HINT_ROW,
            )
            .is_none()
        }));
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
        // The project tab row owns row 0, and the upper panels the rows just
        // below it — neither is a pane.
        assert_eq!(pane_at(&app, screen, &layout, 0, 0), None);
        assert_eq!(pane_at(&app, screen, &layout, 0, 1), None);
        // ...and so do the two chrome rows at the bottom.
        assert_eq!(pane_at(&app, screen, &layout, 0, 39), None);
    }

    #[test]
    fn upper_panel_at_resolves_list_and_diff_by_the_layout_split() {
        let app = app_with_files(vec!["a.rs"]);
        let screen = Rect::new(0, 0, 100, 40);
        let layout = LayoutConfig::default();

        // Row 0 is the project tab row, so the body starts at row 1. The
        // default file_list_pct (25) puts x=0 in the list and x=60 in the diff.
        assert_eq!(upper_panel_at(&app, screen, &layout, 0, 0), None);
        assert_eq!(
            upper_panel_at(&app, screen, &layout, 0, 1),
            Some(Focus::FileList)
        );
        assert_eq!(
            upper_panel_at(&app, screen, &layout, 60, 1),
            Some(Focus::DiffViewer)
        );
        // Below the upper panels: the terminal panel, then the two chrome
        // rows (notice, hint) — none of them is an upper panel.
        assert_eq!(upper_panel_at(&app, screen, &layout, 0, 37), None);
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

        let hit = pane_at(
            &app,
            Rect::new(0, 0, 100, 40),
            &LayoutConfig::default(),
            50,
            30,
        );

        assert_eq!(hit, None);
    }
}
