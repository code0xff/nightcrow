use super::helpers::*;
use crate::mouse::handle_mouse;
use crossterm::event::MouseEventKind;

#[test]
fn handle_mouse_release_follows_the_pressed_pane_when_the_pointer_moves_away() {
    let (mut app, areas) = app_with_two_panes_and_areas();
    let (pressed_id, pressed_rect) = areas[0];
    let (_, other_rect) = areas[1];
    // Only the pressed pane is mouse-aware: any release payload proves
    // routing went to the pressed pane, not the pane under the pointer.
    app.terminal
        .emulators
        .get_mut(&pressed_id)
        .unwrap()
        .process(b"\x1b[?1000h\x1b[?1006h");

    let layout = crate::config::LayoutConfig::default();
    let down = MouseEventKind::Down(crossterm::event::MouseButton::Left);
    let up = MouseEventKind::Up(crossterm::event::MouseButton::Left);
    handle_mouse(
        &mut app,
        test_tab_view(&test_tabs()),
        mouse(down, pressed_rect.x, pressed_rect.y),
        MOUSE_TEST_SCREEN,
        &layout,
    );
    handle_mouse(
        &mut app,
        test_tab_view(&test_tabs()),
        mouse(up, other_rect.x, other_rect.y),
        MOUSE_TEST_SCREEN,
        &layout,
    );

    // The release cell is clamped into the pressed pane's rect.
    let col = other_rect.x.clamp(pressed_rect.x, pressed_rect.right() - 1) - pressed_rect.x + 1;
    let row = other_rect
        .y
        .clamp(pressed_rect.y, pressed_rect.bottom() - 1)
        - pressed_rect.y
        + 1;
    let release = format!("\x1b[<0;{col};{row}m").into_bytes();
    assert_eq!(
        backend_payloads(&app),
        vec![b"\x1b[<0;1;1M".to_vec(), release]
    );
    assert!(app.pending_mouse_press.is_none());
}

#[test]
fn handle_mouse_completes_a_pending_release_even_while_the_repo_modal_is_open() {
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

    // A release must reach the pane that saw the press even when a modal
    // opened in between, and the pending slot must not go stale. The
    // release path runs before any modal guard, so driving it directly is
    // the same code path a real dialog would take.
    handle_mouse(
        &mut app,
        test_tab_view(&test_tabs()),
        mouse(up, rect.x, rect.y),
        MOUSE_TEST_SCREEN,
        &layout,
    );

    assert_eq!(
        backend_payloads(&app),
        vec![b"\x1b[<0;1;1M".to_vec(), b"\x1b[<0;1;1m".to_vec()]
    );
    assert!(app.pending_mouse_press.is_none());
}

#[test]
fn handle_mouse_release_pairs_by_the_stored_press_button() {
    let (mut app, areas) = app_with_two_panes_and_areas();
    let (id, rect) = areas[0];
    app.terminal
        .emulators
        .get_mut(&id)
        .unwrap()
        .process(b"\x1b[?1000h\x1b[?1006h");

    // Press Right, but the terminal reports the release as Left — the
    // legacy encodings don't carry the button on release, so crossterm
    // may fall back to Left. The pane must still see a Right release.
    let layout = crate::config::LayoutConfig::default();
    let down = MouseEventKind::Down(crossterm::event::MouseButton::Right);
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

    assert_eq!(
        backend_payloads(&app),
        vec![b"\x1b[<2;1;1M".to_vec(), b"\x1b[<2;1;1m".to_vec()]
    );
    assert!(app.pending_mouse_press.is_none());
}
