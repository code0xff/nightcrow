use crate::app::tests::{app_with_fake_backend, app_with_files};
use crate::app::{App, Focus};
use crate::backend;
use crate::workspace::Workspace;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers, MouseEvent, MouseEventKind};
use ratatui::layout::Rect;

pub(super) const MOUSE_TEST_SCREEN: Rect = Rect::new(0, 0, 100, 40);

/// Wide screen for hint-row click tests: the longest hint rows overflow
/// the 100-column mouse screen, and a clipped segment is unclickable by
/// design — these tests target segments, so give them room.
pub(super) const HINT_TEST_SCREEN: Rect = Rect::new(0, 0, 300, 40);

pub(super) fn press(code: KeyCode, mods: KeyModifiers) -> KeyEvent {
    KeyEvent::new(code, mods)
}

/// The default leader chord (Ctrl+F). Test apps all use the default, so a
/// standalone constructor avoids borrowing `app` inside a `handle_key`
/// call (which would conflict with the `&mut app` argument).
pub(super) fn leader() -> KeyEvent {
    KeyEvent::new(KeyCode::Char('f'), KeyModifiers::CONTROL)
}

/// Snapshot the byte payloads the app's `FakeBackend` recorded so terminal
/// pass-through and literal-leader tests can assert exact PTY bytes.
pub(super) fn backend_payloads(app: &App) -> Vec<Vec<u8>> {
    app.terminal
        .fake_backend_sent()
        .expect("test app must use a FakeBackend")
}

/// A FakeBackend-backed app with one open terminal pane and terminal
/// focus, ready for PTY pass-through assertions.
pub(super) fn app_with_terminal_pane() -> App {
    let mut app = app_with_fake_backend();
    app.terminal.create_pane().unwrap();
    app.focus = Focus::Terminal;
    app
}

/// A workspace of projects distinguished by `repo_path`, plus the context
/// `apply_project_request` needs. `Open` is the only request that builds a
/// project, so a default config suffices for the rest.
pub(super) fn workspace_on(paths: &[&str]) -> Workspace {
    let project = |p: &str| {
        let mut app = app_with_files(vec!["a.rs"]);
        app.repo_path = p.to_string();
        app
    };
    let mut ws = Workspace::new(leader());
    for p in paths {
        assert!(ws.add(project(p)));
    }
    ws
}

/// A single-project tab row, matching the app these mouse tests drive.
/// Tests that specifically exercise tab clicks build their own list.
pub(super) fn test_tabs() -> Vec<String> {
    vec![".".to_string()]
}

/// A closed dialog to borrow from, so `test_tab_view` can hand out a
/// `Chrome` without referencing a temporary.
static CLOSED_DIALOG: std::sync::LazyLock<crate::ui::status_view::RepoInput> =
    std::sync::LazyLock::new(crate::ui::status_view::RepoInput::default);

pub(super) fn test_tab_view(paths: &[String]) -> crate::ui::Chrome<'_> {
    crate::ui::Chrome {
        repo_paths: paths,
        active: 0,
        repo_input: &CLOSED_DIALOG,
    }
}

pub(super) fn mouse(kind: MouseEventKind, column: u16, row: u16) -> MouseEvent {
    MouseEvent {
        kind,
        column,
        row,
        modifiers: KeyModifiers::NONE,
    }
}

/// A two-pane terminal app plus each pane's content rect under the
/// standard test screen, so mouse tests can aim events at real geometry.
pub(super) fn app_with_two_panes_and_areas() -> (App, Vec<(backend::PaneId, Rect)>) {
    let mut app = app_with_terminal_pane();
    app.terminal.create_pane().unwrap();
    let layout = crate::config::LayoutConfig::default();
    let areas = crate::ui::terminal_content_areas(&app, MOUSE_TEST_SCREEN, &layout);
    assert_eq!(areas.len(), 2);
    (app, areas)
}

/// First x column on the hint row that resolves to `want`, scanning with
/// the same hit-test the mouse handler uses.
pub(super) fn hint_x_for(app: &App, want: crate::ui::HintClick) -> u16 {
    let row = HINT_TEST_SCREEN.height - 1;
    (0..HINT_TEST_SCREEN.width)
        .find(|&x| {
            crate::ui::hint_click_at(app, test_tab_view(&[]), HINT_TEST_SCREEN, x, row)
                == Some(want)
        })
        .expect("expected a clickable hint segment")
}

/// First (x, y) cell resolving to tab-clicks target `want`, scanning with
/// the same hit-test the mouse handler uses.
pub(super) fn tab_xy_for(app: &App, want: usize) -> (u16, u16) {
    let layout = crate::config::LayoutConfig::default();
    for y in 0..MOUSE_TEST_SCREEN.height {
        for x in 0..MOUSE_TEST_SCREEN.width {
            if crate::ui::tab_click_at(app, MOUSE_TEST_SCREEN, &layout, x, y) == Some(want) {
                return (x, y);
            }
        }
    }
    panic!("expected a tab segment targeting pane {want}");
}
