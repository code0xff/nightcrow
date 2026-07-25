use super::helpers::*;
use crate::key_dispatch::handle_key;
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
