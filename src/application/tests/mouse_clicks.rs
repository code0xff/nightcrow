use super::helpers::*;
use crate::app::Focus;
use crate::application::input::dispatch::KeyOutcome;
use crate::application::input::mouse::handle_mouse;
use crossterm::event::MouseEventKind;

#[test]
fn handle_mouse_tab_click_jumps_to_that_pane() {
    let (mut app, _) = app_with_two_panes_and_areas();
    app.terminal.active = 0;
    app.focus = Focus::FileList;
    let (x, y) = tab_xy_for(&app, 1);

    let down = MouseEventKind::Down(crossterm::event::MouseButton::Left);
    handle_mouse(
        &mut app,
        test_tab_view(&test_tabs()),
        mouse(down, x, y),
        MOUSE_TEST_SCREEN,
        &crate::config::LayoutConfig::default(),
    );

    assert_eq!(app.terminal.active, 1);
    assert_eq!(app.focus, Focus::Terminal);
    assert!(
        backend_payloads(&app).is_empty(),
        "a tab click is UI-only; nothing may reach a PTY"
    );
}

#[test]
fn handle_mouse_tab_click_on_hidden_marker_slides_the_window() {
    let mut app = app_with_terminal_pane();
    for _ in 0..5 {
        app.terminal.create_pane_now().unwrap();
    }
    // 6 panes, window of 4: creation leaves pane 5 active, window [2, 6).
    assert_eq!(app.terminal.visible_start, 2);
    // The left ` +2 ` marker targets the nearest hidden pane, index 1.
    let (x, y) = tab_xy_for(&app, 1);

    let down = MouseEventKind::Down(crossterm::event::MouseButton::Left);
    handle_mouse(
        &mut app,
        test_tab_view(&test_tabs()),
        mouse(down, x, y),
        MOUSE_TEST_SCREEN,
        &crate::config::LayoutConfig::default(),
    );

    assert_eq!(app.terminal.active, 1);
    assert_eq!(
        app.terminal.visible_start, 1,
        "revealing the clicked marker's pane must slide the window one slot"
    );
}

#[test]
fn handle_mouse_click_completes_an_armed_swap_with_the_clicked_pane() {
    let (mut app, areas) = app_with_two_panes_and_areas();
    app.terminal.active = 0;
    let first_id = app.terminal.panes[0].id;
    let (clicked_id, rect) = areas[1];
    assert_ne!(clicked_id, first_id);
    app.begin_swap_target();

    let down = MouseEventKind::Down(crossterm::event::MouseButton::Left);
    handle_mouse(
        &mut app,
        test_tab_view(&test_tabs()),
        mouse(down, rect.x, rect.y),
        MOUSE_TEST_SCREEN,
        &crate::config::LayoutConfig::default(),
    );

    // The clicked pane is the swap target, exactly like its digit: the
    // previously active pane moves into the clicked slot and stays active, once
    // the session answers with the new order.
    assert!(!app.awaiting_swap_target());
    app.poll_terminal();
    assert_eq!(app.terminal.panes[1].id, first_id);
    assert_eq!(app.terminal.active, 1);
    assert!(
        backend_payloads(&app).is_empty(),
        "a swap-target click must not be forwarded to any PTY"
    );
}

#[test]
fn handle_mouse_tab_click_completes_an_armed_swap() {
    let (mut app, _) = app_with_two_panes_and_areas();
    app.terminal.active = 0;
    let first_id = app.terminal.panes[0].id;
    app.begin_swap_target();
    let (x, y) = tab_xy_for(&app, 1);

    let down = MouseEventKind::Down(crossterm::event::MouseButton::Left);
    handle_mouse(
        &mut app,
        test_tab_view(&test_tabs()),
        mouse(down, x, y),
        MOUSE_TEST_SCREEN,
        &crate::config::LayoutConfig::default(),
    );

    assert!(!app.awaiting_swap_target());
    app.poll_terminal();
    assert_eq!(app.terminal.panes[1].id, first_id);
    assert_eq!(app.terminal.active, 1);
}

#[test]
fn handle_mouse_press_elsewhere_cancels_an_armed_swap() {
    let (mut app, _) = app_with_two_panes_and_areas();
    app.terminal.active = 0;
    app.begin_swap_target();
    let order_before: Vec<_> = app.terminal.panes.iter().map(|p| p.id).collect();

    // (0, 0) is the header row: it names no pane, so the press must
    // consume-and-disarm without swapping or moving focus — the same
    // rule as a non-digit key.
    let down = MouseEventKind::Down(crossterm::event::MouseButton::Left);
    handle_mouse(
        &mut app,
        test_tab_view(&test_tabs()),
        mouse(down, 0, 0),
        MOUSE_TEST_SCREEN,
        &crate::config::LayoutConfig::default(),
    );

    assert!(!app.awaiting_swap_target());
    let order_after: Vec<_> = app.terminal.panes.iter().map(|p| p.id).collect();
    assert_eq!(order_before, order_after);
    assert_eq!(app.terminal.active, 0);
}

#[test]
fn handle_mouse_hint_click_runs_the_named_leader_command() {
    let mut app = app_with_terminal_pane();
    let panes_before = app.terminal.panes.len();
    let x = hint_x_for(&app, crate::ui::HintClick::Leader('t'));

    let down = MouseEventKind::Down(crossterm::event::MouseButton::Left);
    let outcome = handle_mouse(
        &mut app,
        test_tab_view(&test_tabs()),
        mouse(down, x, HINT_TEST_SCREEN.height - 1),
        HINT_TEST_SCREEN,
        &crate::config::LayoutConfig::default(),
    );

    assert!(matches!(outcome, KeyOutcome::Continue));
    // The pane arrives through the poll the main loop runs every tick.
    app.poll_terminal();
    assert_eq!(
        app.terminal.panes.len(),
        panes_before + 1,
        "clicking `<prefix> t: new pane` must run the same command as the keys"
    );
    assert!(
        !app.prefix_armed(),
        "the synthesized prefix must not linger"
    );
}

#[test]
fn handle_mouse_hint_click_on_the_leader_label_arms_the_prefix() {
    let mut app = app_with_terminal_pane();
    let x = hint_x_for(&app, crate::ui::HintClick::Arm);

    let down = MouseEventKind::Down(crossterm::event::MouseButton::Left);
    let outcome = handle_mouse(
        &mut app,
        test_tab_view(&test_tabs()),
        mouse(down, x, HINT_TEST_SCREEN.height - 1),
        HINT_TEST_SCREEN,
        &crate::config::LayoutConfig::default(),
    );

    assert!(matches!(outcome, KeyOutcome::Continue));
    assert!(
        app.prefix_armed(),
        "clicking `<prefix>: leader` must arm the prefix exactly like the chord"
    );
    assert!(
        backend_payloads(&app).is_empty(),
        "arming is UI-only; nothing may reach a PTY"
    );
}

#[test]
fn handle_mouse_arm_click_then_followup_click_runs_the_command() {
    let mut app = app_with_terminal_pane();
    let panes_before = app.terminal.panes.len();
    let down = MouseEventKind::Down(crossterm::event::MouseButton::Left);
    let row = HINT_TEST_SCREEN.height - 1;

    let x = hint_x_for(&app, crate::ui::HintClick::Arm);
    handle_mouse(
        &mut app,
        test_tab_view(&test_tabs()),
        mouse(down, x, row),
        HINT_TEST_SCREEN,
        &crate::config::LayoutConfig::default(),
    );
    let x = hint_x_for(&app, crate::ui::HintClick::Plain('t'));
    handle_mouse(
        &mut app,
        test_tab_view(&test_tabs()),
        mouse(down, x, row),
        HINT_TEST_SCREEN,
        &crate::config::LayoutConfig::default(),
    );

    app.poll_terminal();
    assert_eq!(
        app.terminal.panes.len(),
        panes_before + 1,
        "arm click + `t` click must open a pane like the key sequence"
    );
    assert!(!app.prefix_armed(), "the follow-up must consume the prefix");
}

#[test]
fn handle_mouse_hint_click_propagates_redraw_from_the_armed_row() {
    let mut app = app_with_terminal_pane();
    app.arm_prefix();
    let x = hint_x_for(&app, crate::ui::HintClick::Plain('r'));

    let down = MouseEventKind::Down(crossterm::event::MouseButton::Left);
    let outcome = handle_mouse(
        &mut app,
        test_tab_view(&test_tabs()),
        mouse(down, x, HINT_TEST_SCREEN.height - 1),
        HINT_TEST_SCREEN,
        &crate::config::LayoutConfig::default(),
    );

    assert!(matches!(outcome, KeyOutcome::Redraw));
    assert!(!app.prefix_armed(), "the follow-up must consume the prefix");
}

#[test]
fn handle_mouse_hint_click_never_quits() {
    let app = app_with_terminal_pane();
    let row = HINT_TEST_SCREEN.height - 1;
    for x in 0..HINT_TEST_SCREEN.width {
        let click = crate::ui::hint_click_at(&app, test_tab_view(&[]), HINT_TEST_SCREEN, x, row);
        assert!(
            !matches!(
                click,
                Some(crate::ui::HintClick::Leader('q')) | Some(crate::ui::HintClick::Plain('q'))
            ),
            "x={x} resolves to a quit click"
        );
    }
}
