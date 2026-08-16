use super::common::{ctrl, key};
use super::*;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers, MouseButton};

#[test]
fn encode_wheel_emits_sgr_press_reports() {
    assert_eq!(encode_wheel(true, 40, 12), b"\x1b[<64;40;12M".to_vec());
    assert_eq!(encode_wheel(false, 40, 12), b"\x1b[<65;40;12M".to_vec());
}

#[test]
fn encode_wheel_clamps_coordinates_to_one_based_origin() {
    // SGR coordinates start at 1; a degenerate 0-sized pane must not
    // produce a `0` that a TUI would read as out of range.
    assert_eq!(encode_wheel(true, 0, 0), b"\x1b[<64;1;1M".to_vec());
}

#[test]
fn encode_wheel_horizontal_uses_sgr_buttons_66_and_67() {
    assert_eq!(
        encode_wheel_horizontal(true, 5, 3),
        b"\x1b[<66;5;3M".to_vec()
    );
    assert_eq!(
        encode_wheel_horizontal(false, 0, 0),
        b"\x1b[<67;1;1M".to_vec()
    );
}

#[test]
fn encode_button_reports_press_and_release_with_real_button_code() {
    assert_eq!(
        encode_button(MouseButton::Left, true, 5, 3),
        b"\x1b[<0;5;3M".to_vec()
    );
    assert_eq!(
        encode_button(MouseButton::Left, false, 5, 3),
        b"\x1b[<0;5;3m".to_vec()
    );
    assert_eq!(
        encode_button(MouseButton::Middle, true, 1, 1),
        b"\x1b[<1;1;1M".to_vec()
    );
    assert_eq!(
        encode_button(MouseButton::Right, false, 80, 24),
        b"\x1b[<2;80;24m".to_vec()
    );
}

#[test]
fn encode_button_clamps_coordinates_to_one_based_origin() {
    assert_eq!(
        encode_button(MouseButton::Left, true, 0, 0),
        b"\x1b[<0;1;1M".to_vec()
    );
}

#[test]
fn encode_arrow_follows_application_cursor_mode() {
    assert_eq!(encode_arrow(true, false), b"\x1b[A".to_vec());
    assert_eq!(encode_arrow(false, false), b"\x1b[B".to_vec());
    assert_eq!(encode_arrow(true, true), b"\x1bOA".to_vec());
    assert_eq!(encode_arrow(false, true), b"\x1bOB".to_vec());
}

#[test]
fn encode_key_uses_application_cursor_mode_for_unmodified_arrows() {
    let up = KeyEvent::new(KeyCode::Up, KeyModifiers::NONE);
    assert_eq!(super::encode_key(up, true), Some(b"\x1bOA".to_vec()));

    let modified = KeyEvent::new(KeyCode::Up, KeyModifiers::CONTROL);
    assert_eq!(
        super::encode_key(modified, true),
        Some(b"\x1b[1;5A".to_vec())
    );
}

#[test]
fn encode_key_emits_xterm_modifier_sequences() {
    use KeyModifiers as M;
    let enc = |code, mods| encode_key(KeyEvent::new(code, mods), false).unwrap();

    // Unmodified cursor/F-keys keep their legacy sequences.
    assert_eq!(enc(KeyCode::Up, M::NONE), b"\x1b[A");
    assert_eq!(enc(KeyCode::F(3), M::NONE), b"\x1bOR");
    assert_eq!(enc(KeyCode::F(5), M::NONE), b"\x1b[15~");
    assert_eq!(enc(KeyCode::PageUp, M::NONE), b"\x1b[5~");

    // Modified keys carry the xterm `1;<mod>` parameter (ctrl=5, shift=2,
    // alt=3).
    assert_eq!(enc(KeyCode::Up, M::CONTROL), b"\x1b[1;5A");
    assert_eq!(enc(KeyCode::Up, M::SHIFT), b"\x1b[1;2A");
    assert_eq!(enc(KeyCode::Left, M::ALT), b"\x1b[1;3D");
    assert_eq!(enc(KeyCode::F(3), M::ALT), b"\x1b[1;3R");
    assert_eq!(enc(KeyCode::F(5), M::CONTROL), b"\x1b[15;5~");
    assert_eq!(enc(KeyCode::PageUp, M::CONTROL), b"\x1b[5;5~");
    assert_eq!(enc(KeyCode::Delete, M::SHIFT), b"\x1b[3;2~");
}

#[test]
fn encode_printable_char() {
    assert_eq!(
        encode_key(key(KeyCode::Char('a')), false),
        Some(b"a".to_vec())
    );
}

#[test]
fn encode_ctrl_c_as_etx() {
    assert_eq!(
        encode_key(ctrl(KeyCode::Char('c')), false),
        Some(vec![0x03])
    );
}

#[test]
fn encode_ctrl_non_ascii_does_not_truncate_to_control_byte() {
    assert_eq!(
        encode_key(ctrl(KeyCode::Char('ŀ')), false),
        Some("ŀ".as_bytes().to_vec())
    );
}

#[test]
fn encode_enter_as_cr() {
    assert_eq!(encode_key(key(KeyCode::Enter), false), Some(vec![b'\r']));
}

#[test]
fn encode_alt_enter_as_esc_cr() {
    // A pane program cannot tell "newline" from "submit" if the modifier is
    // dropped; ESC+CR is the Meta-prefixed form TUIs read as newline.
    assert_eq!(
        encode_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::ALT), false),
        Some(vec![0x1b, b'\r'])
    );
}

#[test]
fn encode_ctrl_enter_as_lf() {
    // Ctrl+J is LF, and Ctrl+Enter is the same chord under another name — the
    // byte a TUI reads as newline. Terminals that report the modifier (Windows'
    // console API, the kitty protocol) get here; the ones that do not already
    // send LF themselves, so both ends agree.
    assert_eq!(encode_key(ctrl(KeyCode::Enter), false), Some(vec![b'\n']));
}

#[test]
fn encode_ctrl_alt_enter_as_esc_lf() {
    assert_eq!(
        encode_key(
            KeyEvent::new(KeyCode::Enter, KeyModifiers::CONTROL | KeyModifiers::ALT),
            false
        ),
        Some(vec![0x1b, b'\n'])
    );
}

#[test]
fn encode_ctrl_space_as_nul() {
    // xterm convention: Ctrl+Space → NUL. The generic `c - '@'` formula
    // wraps for space (0x20 < 0x40), so this case needs special handling.
    assert_eq!(
        encode_key(ctrl(KeyCode::Char(' ')), false),
        Some(vec![0x00])
    );
}

#[test]
fn encode_ctrl_slash_as_us() {
    // Ctrl+/ is conventionally 0x1F (US) on xterm; vim/less/emacs
    // bindings depend on it. Without the explicit mapping the slash
    // fell through as a literal '/' character.
    assert_eq!(
        encode_key(ctrl(KeyCode::Char('/')), false),
        Some(vec![0x1F])
    );
}

#[test]
fn encode_ctrl_question_as_del() {
    // Ctrl+? is conventionally DEL (0x7F).
    assert_eq!(
        encode_key(ctrl(KeyCode::Char('?')), false),
        Some(vec![0x7F])
    );
}

#[test]
fn encode_ctrl_right_bracket_via_formula() {
    // Sanity check: the `c.to_ascii_uppercase() - '@'` formula already
    // covered Ctrl+]. Pin it so a future refactor of the special-case
    // table doesn't accidentally regress it.
    assert_eq!(
        encode_key(ctrl(KeyCode::Char(']')), false),
        Some(vec![0x1D])
    );
}

#[test]
fn encode_ctrl_alt_char_prefixes_esc_to_control_byte() {
    // readline / Emacs convention: Ctrl+Alt+Char → ESC + control byte.
    let key = KeyEvent::new(
        KeyCode::Char('c'),
        KeyModifiers::CONTROL | KeyModifiers::ALT,
    );
    assert_eq!(encode_key(key, false), Some(vec![0x1b, 0x03]));
}
