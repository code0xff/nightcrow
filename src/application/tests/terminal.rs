use super::helpers::*;
use crate::application::input::dispatch::handle_key;
use crossterm::event::{KeyCode, KeyModifiers};

#[test]
fn handle_key_terminal_ctrl_w_passes_through_to_pty() {
    // Ctrl+W (and friends) are prompt-editing keys that must now reach
    // the running program as control bytes instead of closing the pane.
    let mut app = app_with_terminal_pane();

    let _ = handle_key(&mut app, press(KeyCode::Char('w'), KeyModifiers::CONTROL));

    // Ctrl+W encodes to 0x17 (ETB).
    assert_eq!(backend_payloads(&app), vec![vec![0x17]]);
}

#[test]
fn handle_key_terminal_ctrl_app_keys_all_pass_through() {
    // Every former bare-Ctrl app shortcut now reaches the PTY untouched.
    // Ctrl+F is excluded: it is the default leader, so it is intercepted to
    // arm the prefix rather than passed through (see the bare-Ctrl+F test).
    for (c, byte) in [
        ('t', 0x14u8),
        ('w', 0x17),
        ('q', 0x11),
        ('l', 0x0c),
        ('p', 0x10),
        ('o', 0x0f),
    ] {
        let mut app = app_with_terminal_pane();
        let _ = handle_key(&mut app, press(KeyCode::Char(c), KeyModifiers::CONTROL));
        assert_eq!(
            backend_payloads(&app),
            vec![vec![byte]],
            "ctrl+{c} must pass through to the PTY"
        );
    }
}

#[test]
fn handle_key_leader_then_c_cancels_the_pane_recovery() {
    // The fake backend answers a cancel the way a session does — by reporting the
    // pane `cancelled` — so the badge clearing on the next poll is proof the
    // request actually went out.
    let mut app = app_with_terminal_pane();
    let pane = app.terminal.panes[0].id;
    app.terminal.recovery.insert(
        pane,
        crate::runtime::terminal::PaneRecovery {
            state: "waiting_for_reset".to_string(),
            detail: None,
            deadline_epoch: Some(1_700_000_000),
            attempt: 1,
        },
    );
    assert!(app.can_cancel_recovery(), "the hint must be advertised");

    let _ = handle_key(&mut app, leader());
    let _ = handle_key(&mut app, press(KeyCode::Char('c'), KeyModifiers::NONE));
    app.poll_terminal();

    assert!(
        !app.can_cancel_recovery(),
        "`<leader> c` must reach the session"
    );
    assert!(
        backend_payloads(&app).is_empty(),
        "a leader follow-up must never leak into the PTY"
    );
}

#[test]
fn handle_key_bare_c_in_a_terminal_pane_reaches_the_program() {
    // The cancel binding is leader-prefixed precisely so a bare `c` is still
    // ordinary typing in whatever the pane is running.
    let mut app = app_with_terminal_pane();

    let _ = handle_key(&mut app, press(KeyCode::Char('c'), KeyModifiers::NONE));

    assert_eq!(backend_payloads(&app), vec![b"c".to_vec()]);
}
