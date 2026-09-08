use super::helpers::*;
use crate::app::Focus;
use crate::application::input::dispatch::KeyOutcome;
use crate::application::input::mouse::handle_mouse;
use crossterm::event::{KeyModifiers, MouseButton, MouseEventKind};

fn osc8(target: &str) -> Vec<u8> {
    format!("\x1b]8;;{target}\x1b\\open\x1b]8;;\x1b\\").into_bytes()
}

#[test]
fn left_link_click_focuses_pane_and_preempts_mouse_reporting() {
    let (mut app, areas) = app_with_two_panes_and_areas();
    let (id, rect) = areas[0];
    let target = "https://example.test/docs";
    app.terminal
        .emulators
        .get_mut(&id)
        .unwrap()
        .process(&osc8(target));
    app.terminal
        .emulators
        .get_mut(&id)
        .unwrap()
        .process(b"\x1b[?1000h\x1b[?1006h");
    app.focus = Focus::FileList;

    let outcome = handle_mouse(
        &mut app,
        test_tab_view(&test_tabs()),
        mouse(MouseEventKind::Down(MouseButton::Left), rect.x, rect.y),
        MOUSE_TEST_SCREEN,
        &crate::config::LayoutConfig::default(),
    );

    assert_eq!(
        outcome,
        KeyOutcome::OpenLink {
            target: target.into(),
            base: ".".into(),
        }
    );
    assert_eq!(app.focus, Focus::Terminal);
    assert_eq!(app.terminal.active, 0, "ordinary click focus is preserved");
    assert!(backend_payloads(&app).is_empty());
    assert!(app.interaction.pending_mouse_press.is_none());
}

#[test]
fn link_release_does_not_create_an_unpaired_mouse_report() {
    let (mut app, areas) = app_with_two_panes_and_areas();
    let (id, rect) = areas[0];
    app.terminal
        .emulators
        .get_mut(&id)
        .unwrap()
        .process(&osc8("https://example.test"));
    let layout = crate::config::LayoutConfig::default();
    handle_mouse(
        &mut app,
        test_tab_view(&test_tabs()),
        mouse(MouseEventKind::Down(MouseButton::Left), rect.x, rect.y),
        MOUSE_TEST_SCREEN,
        &layout,
    );
    handle_mouse(
        &mut app,
        test_tab_view(&test_tabs()),
        mouse(MouseEventKind::Up(MouseButton::Left), rect.x, rect.y),
        MOUSE_TEST_SCREEN,
        &layout,
    );
    assert!(backend_payloads(&app).is_empty());
}

#[test]
fn selection_modifier_bypasses_link_opening() {
    let (mut app, areas) = app_with_two_panes_and_areas();
    let (id, rect) = areas[0];
    app.terminal
        .emulators
        .get_mut(&id)
        .unwrap()
        .process(&osc8("https://example.test"));
    app.terminal
        .emulators
        .get_mut(&id)
        .unwrap()
        .process(b"\x1b[?1000h\x1b[?1006h");
    let mut event = mouse(MouseEventKind::Down(MouseButton::Left), rect.x, rect.y);
    event.modifiers = KeyModifiers::SHIFT;

    let outcome = handle_mouse(
        &mut app,
        test_tab_view(&test_tabs()),
        event,
        MOUSE_TEST_SCREEN,
        &crate::config::LayoutConfig::default(),
    );
    assert_eq!(outcome, KeyOutcome::Continue);
    assert_eq!(backend_payloads(&app), vec![b"\x1b[<0;1;1M".to_vec()]);
}

#[test]
fn link_click_keeps_a_previous_press_paired_for_release() {
    let (mut app, areas) = app_with_two_panes_and_areas();
    let (first_id, first_rect) = areas[0];
    let (second_id, second_rect) = areas[1];
    app.terminal
        .emulators
        .get_mut(&first_id)
        .unwrap()
        .process(b"\x1b[?1000h\x1b[?1006h");
    app.terminal
        .emulators
        .get_mut(&second_id)
        .unwrap()
        .process(&osc8("https://example.test"));
    let layout = crate::config::LayoutConfig::default();
    handle_mouse(
        &mut app,
        test_tab_view(&test_tabs()),
        mouse(
            MouseEventKind::Down(MouseButton::Left),
            first_rect.x,
            first_rect.y,
        ),
        MOUSE_TEST_SCREEN,
        &layout,
    );
    let outcome = handle_mouse(
        &mut app,
        test_tab_view(&test_tabs()),
        mouse(
            MouseEventKind::Down(MouseButton::Left),
            second_rect.x,
            second_rect.y,
        ),
        MOUSE_TEST_SCREEN,
        &layout,
    );
    assert!(matches!(outcome, KeyOutcome::OpenLink { .. }));
    handle_mouse(
        &mut app,
        test_tab_view(&test_tabs()),
        mouse(
            MouseEventKind::Up(MouseButton::Left),
            second_rect.x,
            second_rect.y,
        ),
        MOUSE_TEST_SCREEN,
        &layout,
    );
    assert_eq!(
        backend_payloads(&app),
        vec![b"\x1b[<0;1;1M".to_vec(), b"\x1b[<0;48;1m".to_vec()]
    );
}
