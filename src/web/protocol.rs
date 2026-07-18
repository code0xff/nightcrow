//! Wire protocol for the web mirror: server→browser screen frames and
//! browser→server input events.
//!
//! **Output** re-uses ratatui's own `CrosstermBackend` to turn a `Buffer`
//! (full frame) or a `Buffer`→`Buffer` diff (incremental) into ANSI. The bytes
//! are therefore byte-identical to what the local terminal receives, and each
//! encoded chunk self-terminates with a style reset (crossterm's `draw`
//! appends one), so chunks concatenate cleanly on a single xterm.js instance.
//!
//! **Input** decodes a small JSON envelope (`{"t":"key"|"mouse"|"paste",…}`)
//! into a crossterm `KeyEvent`/`MouseEvent`/paste string, so browser input runs
//! through the exact same `handle_key`/`handle_mouse`/`handle_paste` routing as
//! local input — a web action can never diverge from the equivalent keypress.

use anyhow::{Result, bail};
use crossterm::event::{
    KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind,
};
use ratatui::backend::{Backend, CrosstermBackend};
use ratatui::buffer::Buffer;
use serde::Deserialize;

/// A decoded browser input event, already lowered to the crossterm types the
/// local input path consumes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WebInputEvent {
    Key(KeyEvent),
    Mouse(MouseEvent),
    Paste(String),
}

/// Encode a full repaint of `current` for a freshly connected client.
///
/// Clears the screen first (so a reconnecting xterm.js drops any stale
/// content), then paints every non-blank cell. Blank default cells are omitted
/// because a cleared terminal already shows them.
pub fn encode_full_frame(current: &Buffer) -> Vec<u8> {
    let blank = Buffer::empty(*current.area());
    let updates = blank.diff(current);
    let mut out = Vec::new();
    {
        let mut backend = CrosstermBackend::new(&mut out);
        // `clear` + `draw` both write ANSI into the Vec; flush on a Vec is a
        // no-op and neither can fail on an in-memory writer.
        let _ = backend.clear();
        let _ = backend.draw(updates.into_iter());
    }
    out
}

/// Encode the incremental update needed to bring a client from `previous` to
/// `current`. Returns an empty Vec when nothing changed (caller skips the send)
/// or when the two buffers have different dimensions — a size change is not a
/// cell-level diff and must be handled by re-sending a full frame instead.
pub fn encode_update(previous: &Buffer, current: &Buffer) -> Vec<u8> {
    if previous.area() != current.area() {
        return Vec::new();
    }
    let updates = previous.diff(current);
    if updates.is_empty() {
        return Vec::new();
    }
    let mut out = Vec::new();
    {
        let mut backend = CrosstermBackend::new(&mut out);
        let _ = backend.draw(updates.into_iter());
    }
    out
}

/// JSON envelope sent by the browser. The `t` tag selects the variant; unknown
/// tags fail to deserialize and are reported as an error by `decode_input`.
#[derive(Debug, Deserialize)]
#[serde(tag = "t", rename_all = "lowercase")]
enum Wire {
    Key {
        /// The browser `KeyboardEvent.key` string (e.g. "a", "Enter", "F5").
        key: String,
        #[serde(default)]
        ctrl: bool,
        #[serde(default)]
        alt: bool,
        #[serde(default)]
        shift: bool,
        #[serde(default)]
        meta: bool,
    },
    Mouse {
        /// "down" | "up" | "move" | "wheel".
        kind: String,
        #[serde(default)]
        button: Option<String>,
        /// 0-based absolute screen cell coordinates.
        col: u16,
        row: u16,
        /// Wheel direction for `kind == "wheel"`: "up"|"down"|"left"|"right".
        #[serde(default)]
        dir: Option<String>,
        #[serde(default)]
        ctrl: bool,
        #[serde(default)]
        alt: bool,
        #[serde(default)]
        shift: bool,
    },
    Paste {
        data: String,
    },
}

/// Decode one JSON input message into a `WebInputEvent`.
///
/// Returns `Ok(None)` for a well-formed message that carries no actionable
/// event — e.g. a modifier-only keypress ("Shift") or an unmappable wheel
/// direction — so the caller can silently drop it. Returns `Err` only for
/// malformed JSON or an unknown message type, which signals a misbehaving or
/// hostile client.
pub fn decode_input(json: &str) -> Result<Option<WebInputEvent>> {
    let wire: Wire = serde_json::from_str(json)
        .map_err(|e| anyhow::anyhow!("invalid web input message: {e}"))?;
    Ok(match wire {
        Wire::Key {
            key,
            ctrl,
            alt,
            shift,
            meta,
        } => decode_key(&key, ctrl, alt, shift, meta).map(WebInputEvent::Key),
        Wire::Mouse {
            kind,
            button,
            col,
            row,
            dir,
            ctrl,
            alt,
            shift,
        } => decode_mouse(&kind, button.as_deref(), col, row, dir.as_deref(), ctrl, alt, shift)
            .map(WebInputEvent::Mouse),
        Wire::Paste { data } => Some(WebInputEvent::Paste(data)),
    })
}

fn modifiers(ctrl: bool, alt: bool, shift: bool, meta: bool) -> KeyModifiers {
    let mut m = KeyModifiers::NONE;
    if ctrl {
        m |= KeyModifiers::CONTROL;
    }
    if alt {
        m |= KeyModifiers::ALT;
    }
    if shift {
        m |= KeyModifiers::SHIFT;
    }
    if meta {
        m |= KeyModifiers::SUPER;
    }
    m
}

/// Map a browser `KeyboardEvent.key` + modifier flags to a crossterm
/// `KeyEvent`. Returns `None` for modifier-only or unidentified keys.
fn decode_key(key: &str, ctrl: bool, alt: bool, shift: bool, meta: bool) -> Option<KeyEvent> {
    let mut mods = modifiers(ctrl, alt, shift, meta);
    let code = match key {
        "Enter" => KeyCode::Enter,
        // Shift+Tab is a distinct key at the terminal level (crossterm reports
        // BackTab, not Tab+Shift); mirror that so it encodes to ESC[Z.
        "Tab" if shift => {
            mods.remove(KeyModifiers::SHIFT);
            KeyCode::BackTab
        }
        "Tab" => KeyCode::Tab,
        "Backspace" => KeyCode::Backspace,
        "Escape" | "Esc" => KeyCode::Esc,
        "ArrowUp" => KeyCode::Up,
        "ArrowDown" => KeyCode::Down,
        "ArrowLeft" => KeyCode::Left,
        "ArrowRight" => KeyCode::Right,
        "Home" => KeyCode::Home,
        "End" => KeyCode::End,
        "PageUp" => KeyCode::PageUp,
        "PageDown" => KeyCode::PageDown,
        "Insert" => KeyCode::Insert,
        "Delete" => KeyCode::Delete,
        " " => KeyCode::Char(' '),
        _ => {
            if let Some(n) = key
                .strip_prefix('F')
                .and_then(|d| (!d.is_empty()).then(|| d.parse::<u8>().ok()).flatten())
            {
                if (1..=24).contains(&n) {
                    return Some(KeyEvent::new(KeyCode::F(n), mods));
                }
                return None;
            }
            // A single Unicode scalar is a printable character. Multi-char key
            // names ("Shift", "Control", "Dead", "Unidentified", …) are not
            // actionable input and are dropped.
            let mut chars = key.chars();
            match (chars.next(), chars.next()) {
                (Some(c), None) => {
                    // A printable glyph already encodes its shifted form, so
                    // drop SHIFT to match how crossterm reports typed chars.
                    mods.remove(KeyModifiers::SHIFT);
                    KeyCode::Char(c)
                }
                _ => return None,
            }
        }
    };
    Some(KeyEvent::new(code, mods))
}

fn mouse_button(button: Option<&str>) -> MouseButton {
    match button {
        Some("right") => MouseButton::Right,
        Some("middle") => MouseButton::Middle,
        // Default to the left button: press/release messages always name a
        // button in practice, and left is the only one the hint/tab/pane click
        // paths act on.
        _ => MouseButton::Left,
    }
}

/// Map a browser mouse message to a crossterm `MouseEvent`. Returns `None` for
/// an unknown kind or an unmappable wheel direction.
#[allow(clippy::too_many_arguments)]
fn decode_mouse(
    kind: &str,
    button: Option<&str>,
    col: u16,
    row: u16,
    dir: Option<&str>,
    ctrl: bool,
    alt: bool,
    shift: bool,
) -> Option<MouseEvent> {
    let event_kind = match kind {
        "down" => MouseEventKind::Down(mouse_button(button)),
        "up" => MouseEventKind::Up(mouse_button(button)),
        "move" => match button {
            Some(b) if b != "none" => MouseEventKind::Drag(mouse_button(button)),
            _ => MouseEventKind::Moved,
        },
        "wheel" => match dir {
            Some("up") => MouseEventKind::ScrollUp,
            Some("down") => MouseEventKind::ScrollDown,
            Some("left") => MouseEventKind::ScrollLeft,
            Some("right") => MouseEventKind::ScrollRight,
            _ => return None,
        },
        _ => return None,
    };
    Some(MouseEvent {
        kind: event_kind,
        column: col,
        row,
        modifiers: modifiers(ctrl, alt, shift, false),
    })
}

/// Reject a payload larger than this before parsing, so a hostile client can't
/// force a huge allocation. Generous enough for any real paste.
pub const MAX_INPUT_MESSAGE_BYTES: usize = 1 << 20;

/// Guard used by the server before calling `decode_input`: an oversized frame
/// is refused outright.
pub fn ensure_input_size(len: usize) -> Result<()> {
    if len > MAX_INPUT_MESSAGE_BYTES {
        bail!("web input message of {len} bytes exceeds the {MAX_INPUT_MESSAGE_BYTES}-byte cap");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
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
        assert!(bytes.windows(4).any(|w| w == b"\x1b[2J"), "full frame must clear");
        assert!(
            bytes.windows(2).any(|w| w == b"hi"),
            "full frame must paint the cell content"
        );
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
        assert!(!text.contains("car"), "unchanged prefix must not be repainted");
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
        assert!(bytes.contains(&0x1b), "styled output must contain escape codes");
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
    fn decode_ctrl_q_matches_default_leader() {
        // The default leader is Ctrl+Q; a browser ctrl+q must decode to the
        // identical KeyEvent so the leader arms from the web too.
        let ev = decode_input(r#"{"t":"key","key":"q","ctrl":true}"#)
            .unwrap()
            .unwrap();
        assert_eq!(
            ev,
            WebInputEvent::Key(KeyEvent::new(KeyCode::Char('q'), KeyModifiers::CONTROL))
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
            assert_eq!(ev, WebInputEvent::Key(KeyEvent::new(code, KeyModifiers::NONE)), "for {json}");
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
        assert!(decode_input(r#"{"t":"key","key":"Shift"}"#).unwrap().is_none());
        assert!(decode_input(r#"{"t":"key","key":"Control"}"#).unwrap().is_none());
        assert!(decode_input(r#"{"t":"key","key":"Dead"}"#).unwrap().is_none());
    }

    #[test]
    fn decode_out_of_range_function_key_is_dropped() {
        assert!(decode_input(r#"{"t":"key","key":"F99"}"#).unwrap().is_none());
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
        assert!(decode_input(r#"{"t":"key"}"#).is_err(), "missing required field");
    }

    #[test]
    fn input_size_guard_rejects_oversized() {
        assert!(ensure_input_size(MAX_INPUT_MESSAGE_BYTES).is_ok());
        assert!(ensure_input_size(MAX_INPUT_MESSAGE_BYTES + 1).is_err());
    }
}
