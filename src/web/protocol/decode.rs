use super::WebInputEvent;
use anyhow::{Result, bail};
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind};
use serde::Deserialize;

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
        } => decode_mouse(
            &kind,
            button.as_deref(),
            col,
            row,
            dir.as_deref(),
            ctrl,
            alt,
            shift,
        )
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
