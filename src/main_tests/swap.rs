use super::helpers::*;
use crate::app::Focus;
use crate::key_dispatch::handle_key;
use crossterm::event::{KeyCode, KeyModifiers};

#[test]
fn handle_key_leader_s_then_digit_swaps_active_pane() {
    // `<leader> s 5` swaps the active pane with pane index 2 (digit 5
    // mirrors F5 → pane 2) and moves focus to follow it.
    let mut app = app_with_terminal_pane();
    for i in 1..3 {
        app.terminal
            .create_pane_with(None, Some(&format!("pane{i}")))
            .unwrap();
    }
    assert_eq!(app.terminal.panes.len(), 3);
    app.switch_pane(0);
    let moving_id = app.terminal.panes[0].id;
    let target_id = app.terminal.panes[2].id;

    // `<leader> s` arms swap mode without acting.
    let _ = handle_key(&mut app, leader());
    let _ = handle_key(&mut app, press(KeyCode::Char('s'), KeyModifiers::NONE));
    assert!(app.awaiting_swap_target(), "leader+s must arm swap mode");
    assert!(!app.prefix_armed(), "swap mode must clear the prefix");

    // The digit resolves the swap.
    let _ = handle_key(&mut app, press(KeyCode::Char('5'), KeyModifiers::NONE));
    assert!(
        !app.awaiting_swap_target(),
        "the digit must disarm swap mode"
    );
    assert_eq!(app.terminal.panes[0].id, target_id);
    assert_eq!(app.terminal.panes[2].id, moving_id);
    assert_eq!(app.terminal.active, 2, "focus follows the moved pane");
    assert!(
        backend_payloads(&app).is_empty(),
        "a consumed swap digit must not reach the PTY"
    );
}

#[test]
fn handle_key_leader_s_esc_cancels_without_swapping() {
    let mut app = app_with_terminal_pane();
    app.terminal.create_pane_with(None, Some("two")).unwrap();
    app.switch_pane(0);
    let order: Vec<_> = app.terminal.panes.iter().map(|p| p.id).collect();

    let _ = handle_key(&mut app, leader());
    let _ = handle_key(&mut app, press(KeyCode::Char('s'), KeyModifiers::NONE));
    let _ = handle_key(&mut app, press(KeyCode::Esc, KeyModifiers::NONE));

    assert!(!app.awaiting_swap_target());
    assert_eq!(app.terminal.active, 0);
    let after: Vec<_> = app.terminal.panes.iter().map(|p| p.id).collect();
    assert_eq!(order, after, "esc must leave pane order unchanged");
}

#[test]
fn handle_key_leader_s_non_digit_cancels() {
    // A non-pane follow-up (e.g. a letter) cancels swap mode and is
    // consumed rather than swapping or reaching the PTY.
    let mut app = app_with_terminal_pane();
    app.terminal.create_pane_with(None, Some("two")).unwrap();
    app.switch_pane(0);
    let order: Vec<_> = app.terminal.panes.iter().map(|p| p.id).collect();

    let _ = handle_key(&mut app, leader());
    let _ = handle_key(&mut app, press(KeyCode::Char('s'), KeyModifiers::NONE));
    let _ = handle_key(&mut app, press(KeyCode::Char('z'), KeyModifiers::NONE));

    assert!(!app.awaiting_swap_target());
    let after: Vec<_> = app.terminal.panes.iter().map(|p| p.id).collect();
    assert_eq!(order, after);
    assert!(backend_payloads(&app).is_empty());
}

#[test]
fn handle_key_leader_s_without_terminal_focus_does_not_arm() {
    let mut app = app_with_terminal_pane();
    app.terminal.create_pane_with(None, Some("two")).unwrap();
    app.focus = Focus::FileList;

    let _ = handle_key(&mut app, leader());
    let _ = handle_key(&mut app, press(KeyCode::Char('s'), KeyModifiers::NONE));

    assert!(
        !app.awaiting_swap_target(),
        "leader+s must not arm swap mode without terminal focus"
    );
    assert!(
        !app.prefix_armed(),
        "the follow-up must still disarm the prefix"
    );
    assert!(
        backend_payloads(&app).is_empty(),
        "the consumed chord must not reach the PTY"
    );
}

#[test]
fn handle_key_leader_s_with_single_pane_does_not_arm() {
    let mut app = app_with_terminal_pane();
    assert_eq!(app.terminal.panes.len(), 1);

    let _ = handle_key(&mut app, leader());
    let _ = handle_key(&mut app, press(KeyCode::Char('s'), KeyModifiers::NONE));

    assert!(
        !app.awaiting_swap_target(),
        "leader+s must not arm swap mode with a single pane"
    );
    assert!(backend_payloads(&app).is_empty());
}
