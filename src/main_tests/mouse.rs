use super::helpers::*;
use crate::app::Focus;
use crate::mouse::{dispatch_mouse, handle_mouse};
use crate::workspace::Workspace;
use crossterm::event::MouseEventKind;

#[test]
fn handle_mouse_click_focuses_the_pane_under_the_pointer() {
    let (mut app, areas) = app_with_two_panes_and_areas();
    app.focus = Focus::FileList;
    let (first_id, rect) = areas[0];
    let first_idx = app
        .terminal
        .panes
        .iter()
        .position(|p| p.id == first_id)
        .unwrap();
    assert_ne!(app.terminal.active, first_idx, "click must change focus");

    let kind = MouseEventKind::Down(crossterm::event::MouseButton::Left);
    handle_mouse(
        &mut app,
        test_tab_view(&test_tabs()),
        mouse(kind, rect.x, rect.y),
        MOUSE_TEST_SCREEN,
        &crate::config::LayoutConfig::default(),
    );

    assert_eq!(app.terminal.active, first_idx);
    assert_eq!(app.focus, Focus::Terminal);
    assert!(
        backend_payloads(&app).is_empty(),
        "a plain shell never claimed the mouse, so the click byte stream \
         must stay empty"
    );
}

#[test]
fn handle_mouse_forwards_press_and_release_to_a_mouse_reporting_pane() {
    let (mut app, areas) = app_with_two_panes_and_areas();
    let (id, rect) = areas[0];
    app.terminal
        .emulators
        .get_mut(&id)
        .unwrap()
        .process(b"\x1b[?1000h\x1b[?1006h");

    let layout = crate::config::LayoutConfig::default();
    let down = MouseEventKind::Down(crossterm::event::MouseButton::Left);
    let up = MouseEventKind::Up(crossterm::event::MouseButton::Left);
    handle_mouse(
        &mut app,
        test_tab_view(&test_tabs()),
        mouse(down, rect.x, rect.y),
        MOUSE_TEST_SCREEN,
        &layout,
    );
    handle_mouse(
        &mut app,
        test_tab_view(&test_tabs()),
        mouse(up, rect.x, rect.y),
        MOUSE_TEST_SCREEN,
        &layout,
    );

    // The pane's top-left content cell is SGR cell (1, 1).
    assert_eq!(
        backend_payloads(&app),
        vec![b"\x1b[<0;1;1M".to_vec(), b"\x1b[<0;1;1m".to_vec()]
    );
}

#[test]
fn handle_mouse_click_focuses_the_upper_panels() {
    let (mut app, _) = app_with_two_panes_and_areas();
    assert_eq!(app.focus, Focus::Terminal);
    let layout = crate::config::LayoutConfig::default();
    let down = MouseEventKind::Down(crossterm::event::MouseButton::Left);

    // Row 1 is the first body row; x=0 is the list, x=60 the diff.
    handle_mouse(
        &mut app,
        test_tab_view(&test_tabs()),
        mouse(down, 0, 1),
        MOUSE_TEST_SCREEN,
        &layout,
    );
    assert_eq!(app.focus, Focus::FileList);

    handle_mouse(
        &mut app,
        test_tab_view(&test_tabs()),
        mouse(down, 60, 1),
        MOUSE_TEST_SCREEN,
        &layout,
    );
    assert_eq!(app.focus, Focus::DiffViewer);

    assert!(
        backend_payloads(&app).is_empty(),
        "an upper-panel click must not write to any PTY"
    );
}

#[test]
fn handle_mouse_is_inert_while_a_search_overlay_is_open() {
    let (mut app, areas) = app_with_two_panes_and_areas();
    app.focus = Focus::FileList;
    app.status_view.search_active = true;
    let (_, rect) = areas[0];
    let active_before = app.terminal.active;

    let kind = MouseEventKind::Down(crossterm::event::MouseButton::Left);
    handle_mouse(
        &mut app,
        test_tab_view(&test_tabs()),
        mouse(kind, rect.x, rect.y),
        MOUSE_TEST_SCREEN,
        &crate::config::LayoutConfig::default(),
    );

    assert_eq!(
        app.focus,
        Focus::FileList,
        "a search overlay owns the mouse exactly like it owns keys"
    );
    assert_eq!(app.terminal.active, active_before);
    assert!(backend_payloads(&app).is_empty());
}

#[test]
fn handle_mouse_drops_a_release_with_no_pending_press() {
    let (mut app, areas) = app_with_two_panes_and_areas();
    let (id, rect) = areas[0];
    app.terminal
        .emulators
        .get_mut(&id)
        .unwrap()
        .process(b"\x1b[?1000h\x1b[?1006h");

    let up = MouseEventKind::Up(crossterm::event::MouseButton::Left);
    handle_mouse(
        &mut app,
        test_tab_view(&test_tabs()),
        mouse(up, rect.x, rect.y),
        MOUSE_TEST_SCREEN,
        &crate::config::LayoutConfig::default(),
    );

    assert!(
        backend_payloads(&app).is_empty(),
        "a pane must not receive a release it never got a press for"
    );
}

#[test]
fn handle_mouse_ignores_events_outside_pane_content() {
    let (mut app, _) = app_with_two_panes_and_areas();
    app.focus = Focus::FileList;
    let active_before = app.terminal.active;

    let kind = MouseEventKind::Down(crossterm::event::MouseButton::Left);
    // (0, 0) is the upper header row, never pane content.
    handle_mouse(
        &mut app,
        test_tab_view(&test_tabs()),
        mouse(kind, 0, 0),
        MOUSE_TEST_SCREEN,
        &crate::config::LayoutConfig::default(),
    );

    assert_eq!(app.focus, Focus::FileList);
    assert_eq!(app.terminal.active, active_before);
}

#[test]
fn mouse_is_inert_while_the_repo_dialog_is_open() {
    let (app, areas) = app_with_two_panes_and_areas();
    let mut ws = Workspace::new(leader());
    ws.add(app);
    ws.active_mut().unwrap().focus = Focus::FileList;
    ws.start_repo_input();
    let (_, rect) = areas[0];
    let active_before = ws.active().unwrap().terminal.active;

    let kind = MouseEventKind::Down(crossterm::event::MouseButton::Left);
    let tabs = test_tabs();
    dispatch_mouse(
        &mut ws,
        test_tab_view(&tabs),
        mouse(kind, rect.x, rect.y),
        MOUSE_TEST_SCREEN,
        &crate::config::LayoutConfig::default(),
        true,
    );

    let app = ws.active().unwrap();
    assert_eq!(app.focus, Focus::FileList, "a modal owns all input");
    assert_eq!(app.terminal.active, active_before);
}

#[test]
fn handle_mouse_wheel_scrolls_the_pane_under_the_pointer_not_the_active_one() {
    let (mut app, areas) = app_with_two_panes_and_areas();
    let (id, rect) = areas[0];
    let idx = app.terminal.panes.iter().position(|p| p.id == id).unwrap();
    let active_before = app.terminal.active;
    assert_ne!(active_before, idx, "wheel must not require focus");
    // Overflow the pane so its emulator has scrollback to move into.
    app.terminal.resize_visible_panes(&[(id, 10, 40)]);
    let output = (0..20).fold(Vec::new(), |mut out, i| {
        out.extend_from_slice(format!("line{i}\r\n").as_bytes());
        out
    });
    app.terminal
        .emulators
        .get_mut(&id)
        .unwrap()
        .process(&output);

    handle_mouse(
        &mut app,
        test_tab_view(&test_tabs()),
        mouse(MouseEventKind::ScrollUp, rect.x, rect.y),
        MOUSE_TEST_SCREEN,
        &crate::config::LayoutConfig::default(),
    );

    assert_eq!(app.terminal.scroll.get(&id).copied(), Some(3));
    assert_eq!(
        app.terminal.active, active_before,
        "a wheel scroll must not steal focus"
    );
}
