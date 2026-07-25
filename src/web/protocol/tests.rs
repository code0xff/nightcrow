use super::*;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind};
use ratatui::layout::Rect;
use ratatui::style::{Color, Style};

fn buf(w: u16, h: u16) -> Buffer {
    Buffer::empty(Rect::new(0, 0, w, h))
}

#[test]
fn full_frame_clears_then_paints_content() {
    let mut b = buf(10, 2);
    b.set_string(0, 0, "hi", Style::default());
    let bytes = encode_full_frame(&b);
    // Clears first (ESC[2J) so a reconnecting client drops stale content,
    // and the painted text is present.
    assert!(
        bytes.windows(4).any(|w| w == b"\x1b[2J"),
        "full frame must clear"
    );
    assert!(
        bytes.windows(2).any(|w| w == b"hi"),
        "full frame must paint the cell content"
    );
}

#[test]
fn cursor_at_a_cell_moves_and_shows_in_one_based_coords() {
    let bytes = encode_cursor(Some(Position::new(3, 7)));
    assert_eq!(bytes, b"\x1b[8;4H\x1b[?25h".to_vec());
}

#[test]
fn absent_cursor_hides_it() {
    assert_eq!(encode_cursor(None), b"\x1b[?25l".to_vec());
}

#[test]
fn update_emits_only_changed_cells() {
    let mut prev = buf(10, 1);
    prev.set_string(0, 0, "cat", Style::default());
    let mut next = prev.clone();
    next.set_string(0, 0, "car", Style::default());

    let bytes = encode_update(&prev, &next);
    let text = String::from_utf8_lossy(&bytes);
    // Only the third column changed ('t' -> 'r'); the update must carry the
    // new glyph but not repaint the unchanged prefix.
    assert!(text.contains('r'), "changed glyph must be present");
    assert!(
        !text.contains("car"),
        "unchanged prefix must not be repainted"
    );
}

#[test]
fn update_is_empty_when_nothing_changed() {
    let b = {
        let mut b = buf(4, 1);
        b.set_string(0, 0, "same", Style::default());
        b
    };
    assert!(
        encode_update(&b, &b.clone()).is_empty(),
        "an identical frame produces no bytes"
    );
}

#[test]
fn update_is_empty_on_size_change() {
    let prev = buf(4, 1);
    let next = buf(6, 1);
    assert!(
        encode_update(&prev, &next).is_empty(),
        "a size change is not a cell diff; caller must send a full frame"
    );
}

#[test]
fn full_frame_carries_color_styling() {
    let mut b = buf(4, 1);
    b.set_string(0, 0, "x", Style::default().fg(Color::Red));
    let bytes = encode_full_frame(&b);
    // Crossterm encodes a red foreground via an SGR sequence; assert some
    // SGR + the glyph made it through (exact code is crossterm's concern).
    assert!(
        bytes.contains(&0x1b),
        "styled output must contain escape codes"
    );
    assert!(bytes.contains(&b'x'));
}

#[test]
fn decode_plain_char_key() {
    let ev = decode_input(r#"{"t":"key","key":"a"}"#).unwrap().unwrap();
    assert_eq!(
        ev,
        WebInputEvent::Key(KeyEvent::new(KeyCode::Char('a'), KeyModifiers::NONE))
    );
}

#[test]
fn decode_ctrl_f_matches_default_leader() {
    // The default leader is Ctrl+F; a browser ctrl+f must decode to the
    // identical KeyEvent so the leader arms from the web too.
    let ev = decode_input(r#"{"t":"key","key":"f","ctrl":true}"#)
        .unwrap()
        .unwrap();
    assert_eq!(
        ev,
        WebInputEvent::Key(KeyEvent::new(KeyCode::Char('f'), KeyModifiers::CONTROL))
    );
}

#[test]
fn decode_uppercase_char_drops_shift() {
    // The glyph already encodes shift; SHIFT is dropped to match crossterm.
    let ev = decode_input(r#"{"t":"key","key":"A","shift":true}"#)
        .unwrap()
        .unwrap();
    assert_eq!(
        ev,
        WebInputEvent::Key(KeyEvent::new(KeyCode::Char('A'), KeyModifiers::NONE))
    );
}

#[test]
fn decode_named_keys() {
    let cases = [
        (r#"{"t":"key","key":"Enter"}"#, KeyCode::Enter),
        (r#"{"t":"key","key":"Escape"}"#, KeyCode::Esc),
        (r#"{"t":"key","key":"ArrowUp"}"#, KeyCode::Up),
        (r#"{"t":"key","key":"PageDown"}"#, KeyCode::PageDown),
        (r#"{"t":"key","key":"F5"}"#, KeyCode::F(5)),
        (r#"{"t":"key","key":" "}"#, KeyCode::Char(' ')),
    ];
    for (json, code) in cases {
        let ev = decode_input(json).unwrap().unwrap();
        assert_eq!(
            ev,
            WebInputEvent::Key(KeyEvent::new(code, KeyModifiers::NONE)),
            "for {json}"
        );
    }
}

#[test]
fn decode_shift_arrow_preserves_shift_for_reserved_scroll_keys() {
    // Shift+Arrow is a reserved nightcrow key (terminal scroll / focus
    // cycle); the SHIFT modifier must survive on non-char keys.
    let ev = decode_input(r#"{"t":"key","key":"ArrowUp","shift":true}"#)
        .unwrap()
        .unwrap();
    assert_eq!(
        ev,
        WebInputEvent::Key(KeyEvent::new(KeyCode::Up, KeyModifiers::SHIFT))
    );
}

#[test]
fn decode_shift_tab_becomes_backtab() {
    let ev = decode_input(r#"{"t":"key","key":"Tab","shift":true}"#)
        .unwrap()
        .unwrap();
    assert_eq!(
        ev,
        WebInputEvent::Key(KeyEvent::new(KeyCode::BackTab, KeyModifiers::NONE))
    );
}

#[test]
fn decode_letter_f_is_a_char_not_a_function_key() {
    let ev = decode_input(r#"{"t":"key","key":"F"}"#).unwrap().unwrap();
    assert_eq!(
        ev,
        WebInputEvent::Key(KeyEvent::new(KeyCode::Char('F'), KeyModifiers::NONE))
    );
}

#[test]
fn decode_modifier_only_key_is_dropped() {
    assert!(
        decode_input(r#"{"t":"key","key":"Shift"}"#)
            .unwrap()
            .is_none()
    );
    assert!(
        decode_input(r#"{"t":"key","key":"Control"}"#)
            .unwrap()
            .is_none()
    );
    assert!(
        decode_input(r#"{"t":"key","key":"Dead"}"#)
            .unwrap()
            .is_none()
    );
}

#[test]
fn decode_out_of_range_function_key_is_dropped() {
    assert!(
        decode_input(r#"{"t":"key","key":"F99"}"#)
            .unwrap()
            .is_none()
    );
}

#[test]
fn decode_mouse_down_and_wheel_and_up() {
    let down = decode_input(r#"{"t":"mouse","kind":"down","button":"left","col":3,"row":4}"#)
        .unwrap()
        .unwrap();
    assert_eq!(
        down,
        WebInputEvent::Mouse(MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Left),
            column: 3,
            row: 4,
            modifiers: KeyModifiers::NONE,
        })
    );

    let wheel = decode_input(r#"{"t":"mouse","kind":"wheel","dir":"up","col":1,"row":1}"#)
        .unwrap()
        .unwrap();
    assert_eq!(
        wheel,
        WebInputEvent::Mouse(MouseEvent {
            kind: MouseEventKind::ScrollUp,
            column: 1,
            row: 1,
            modifiers: KeyModifiers::NONE,
        })
    );

    let up = decode_input(r#"{"t":"mouse","kind":"up","button":"right","col":2,"row":2}"#)
        .unwrap()
        .unwrap();
    assert_eq!(
        up,
        WebInputEvent::Mouse(MouseEvent {
            kind: MouseEventKind::Up(MouseButton::Right),
            column: 2,
            row: 2,
            modifiers: KeyModifiers::NONE,
        })
    );
}

#[test]
fn decode_wheel_without_direction_is_dropped() {
    assert!(
        decode_input(r#"{"t":"mouse","kind":"wheel","col":1,"row":1}"#)
            .unwrap()
            .is_none()
    );
}

#[test]
fn decode_paste() {
    let ev = decode_input(r#"{"t":"paste","data":"hello\nworld"}"#)
        .unwrap()
        .unwrap();
    assert_eq!(ev, WebInputEvent::Paste("hello\nworld".to_string()));
}

#[test]
fn decode_rejects_malformed_and_unknown() {
    assert!(decode_input("not json").is_err());
    assert!(decode_input(r#"{"t":"explode"}"#).is_err());
    assert!(
        decode_input(r#"{"t":"key"}"#).is_err(),
        "missing required field"
    );
}

#[test]
fn input_size_guard_rejects_oversized() {
    assert!(ensure_input_size(MAX_INPUT_MESSAGE_BYTES).is_ok());
    assert!(ensure_input_size(MAX_INPUT_MESSAGE_BYTES + 1).is_err());
}
