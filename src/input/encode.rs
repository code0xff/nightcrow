use crossterm::event::{KeyCode, KeyEvent, KeyModifiers, MouseButton};

/// Encode a crossterm KeyEvent as VT100/ANSI bytes for terminal pass-through.
/// `app_cursor` is the active pane's DECCKM state; it changes unmodified arrow
/// keys from CSI (`ESC [ A`) to SS3 (`ESC O A`). Modified arrows keep xterm's
/// CSI modifier form.
pub fn encode_key(key: KeyEvent, app_cursor: bool) -> Option<Vec<u8>> {
    let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
    let alt = key.modifiers.contains(KeyModifiers::ALT);

    match key.code {
        KeyCode::Char(c) => {
            if ctrl && c.is_ascii() {
                // Ctrl chords outside the `letter - '@'` formula need explicit
                // xterm-convention mappings: Ctrl+Space → NUL (the formula
                // wraps because ' ' < '@'), Ctrl+/ → 0x1F (US), Ctrl+? → 0x7F.
                let b = match c {
                    ' ' => Some(0x00),
                    '/' => Some(0x1F),
                    '?' => Some(0x7F),
                    _ => {
                        let v = (c.to_ascii_uppercase() as u8).wrapping_sub(b'@');
                        (v < 32).then_some(v)
                    }
                };
                if let Some(b) = b {
                    // Ctrl+Alt+Char encodes as ESC + control byte (matches
                    // readline / Emacs expectations).
                    return Some(if alt { vec![0x1b, b] } else { vec![b] });
                }
            }
            if alt {
                let mut bytes = vec![0x1b];
                let mut enc = [0u8; 4];
                bytes.extend_from_slice(c.encode_utf8(&mut enc).as_bytes());
                return Some(bytes);
            }
            let mut enc = [0u8; 4];
            Some(c.encode_utf8(&mut enc).as_bytes().to_vec())
        }
        // Alt+Enter carries the Meta prefix like Alt+Char does: terminal UIs
        // read ESC+CR as "insert a newline, don't submit" (Claude Code binds
        // its newline to it), so dropping the modifier made every Alt+Enter
        // submit instead. Ctrl+Enter is LF for the same reason — a terminal
        // that can report the modifier delivers `Enter + CONTROL`, and
        // encoding that as CR would submit the line on exactly those
        // platforms.
        KeyCode::Enter => {
            let byte = if ctrl { b'\n' } else { b'\r' };
            Some(if alt { vec![0x1b, byte] } else { vec![byte] })
        }
        KeyCode::Backspace => Some(vec![0x7f]),
        KeyCode::Delete => Some(csi_tilde(3, key.modifiers)),
        KeyCode::Esc => Some(vec![0x1b]),
        KeyCode::Tab => Some(vec![b'\t']),
        KeyCode::BackTab => Some(b"\x1b[Z".to_vec()),
        KeyCode::Up => Some(cursor_arrow(b'A', key.modifiers, app_cursor)),
        KeyCode::Down => Some(cursor_arrow(b'B', key.modifiers, app_cursor)),
        KeyCode::Right => Some(cursor_arrow(b'C', key.modifiers, app_cursor)),
        KeyCode::Left => Some(cursor_arrow(b'D', key.modifiers, app_cursor)),
        KeyCode::Home => Some(csi_cursor(b'H', key.modifiers)),
        KeyCode::End => Some(csi_cursor(b'F', key.modifiers)),
        KeyCode::PageUp => Some(csi_tilde(5, key.modifiers)),
        KeyCode::PageDown => Some(csi_tilde(6, key.modifiers)),
        KeyCode::F(n) => encode_function_key(n, key.modifiers),
        _ => None,
    }
}

/// SGR (1006) mouse button code for a wheel-up event. Wheel-down is one more.
/// Bit 6 (64) marks the button as a wheel rather than a click.
const SGR_WHEEL_UP: u8 = 64;

/// Encode a mouse wheel notch as an SGR (1006) mouse report; `col`/`row` are
/// 1-based cells. A wheel notch has no release event, so a single `M` (press)
/// report is the whole sequence — unlike a click, which xterm follows with `m`.
pub fn encode_wheel(up: bool, col: u16, row: u16) -> Vec<u8> {
    let button = if up { SGR_WHEEL_UP } else { SGR_WHEEL_UP + 1 };
    format!("\x1b[<{button};{};{}M", col.max(1), row.max(1)).into_bytes()
}

/// Encode a horizontal wheel notch as an SGR (1006) mouse report: button 66
/// is wheel-left, 67 wheel-right. Horizontal wheel has no scrollback or
/// arrow-key analog, so this only ever targets a pane that claimed the mouse.
pub fn encode_wheel_horizontal(left: bool, col: u16, row: u16) -> Vec<u8> {
    let button: u8 = if left {
        SGR_WHEEL_UP + 2
    } else {
        SGR_WHEEL_UP + 3
    };
    format!("\x1b[<{button};{};{}M", col.max(1), row.max(1)).into_bytes()
}

/// Encode a mouse button press or release as an SGR (1006) mouse report with
/// 1-based pane-local cell coordinates. SGR keeps the real button code on
/// release and marks it with a final `m` instead of `M` — unlike legacy X10,
/// which collapses every release to button 3.
pub fn encode_button(button: MouseButton, press: bool, col: u16, row: u16) -> Vec<u8> {
    let code: u8 = match button {
        MouseButton::Left => 0,
        MouseButton::Middle => 1,
        MouseButton::Right => 2,
    };
    let final_byte = if press { 'M' } else { 'm' };
    format!("\x1b[<{code};{};{}{final_byte}", col.max(1), row.max(1)).into_bytes()
}

/// Encode a bare Up/Down arrow. `app_cursor` selects the SS3 form (`ESC O A`)
/// that DECCKM-enabled programs expect over the default CSI form (`ESC [ A`).
pub fn encode_arrow(up: bool, app_cursor: bool) -> Vec<u8> {
    let final_byte = if up { b'A' } else { b'B' };
    cursor_arrow(final_byte, KeyModifiers::NONE, app_cursor)
}

/// xterm modifier parameter for CSI sequences: `1 + (shift=1 | alt=2 | ctrl=4 |
/// meta=8)`. Returns `None` when no modifier is held, signalling that the
/// legacy unparametrized escape sequence should be used instead.
fn xterm_modifier_param(mods: KeyModifiers) -> Option<u8> {
    let mut bits = 0u8;
    if mods.contains(KeyModifiers::SHIFT) {
        bits |= 1;
    }
    if mods.contains(KeyModifiers::ALT) {
        bits |= 2;
    }
    if mods.contains(KeyModifiers::CONTROL) {
        bits |= 4;
    }
    if mods.intersects(KeyModifiers::SUPER | KeyModifiers::HYPER | KeyModifiers::META) {
        bits |= 8;
    }
    (bits != 0).then_some(bits + 1)
}

/// Encode a cursor/edit key of the `ESC [ <final>` family, inserting the
/// `1;<mod>` parameters when a modifier is held so the PTY program sees e.g.
/// `Ctrl+Up` (`ESC[1;5A`) instead of a bare `Up`.
fn csi_cursor(final_byte: u8, mods: KeyModifiers) -> Vec<u8> {
    match xterm_modifier_param(mods) {
        Some(m) => {
            let mut bytes = format!("\x1b[1;{m}").into_bytes();
            bytes.push(final_byte);
            bytes
        }
        None => vec![0x1b, b'[', final_byte],
    }
}

fn cursor_arrow(final_byte: u8, mods: KeyModifiers, app_cursor: bool) -> Vec<u8> {
    if mods.is_empty() && app_cursor {
        vec![0x1b, b'O', final_byte]
    } else {
        csi_cursor(final_byte, mods)
    }
}

/// Encode a `ESC [ <n> ~` edit key (PageUp/PageDown/Delete), adding the
/// `;<mod>` parameter when a modifier is held.
fn csi_tilde(n: u8, mods: KeyModifiers) -> Vec<u8> {
    match xterm_modifier_param(mods) {
        Some(m) => format!("\x1b[{n};{m}~").into_bytes(),
        None => format!("\x1b[{n}~").into_bytes(),
    }
}

/// Encode an F-key. F1–F4 use the SS3 form (`ESC O P..S`) when unmodified and
/// the CSI form (`ESC[1;<mod>P..S`) when modified; F5–F12 use the tilde form.
fn encode_function_key(n: u8, mods: KeyModifiers) -> Option<Vec<u8>> {
    let param = xterm_modifier_param(mods);
    let seq = match n {
        1..=4 => {
            let final_byte = b"PQRS"[(n - 1) as usize];
            match param {
                Some(m) => {
                    let mut bytes = format!("\x1b[1;{m}").into_bytes();
                    bytes.push(final_byte);
                    bytes
                }
                None => vec![0x1b, b'O', final_byte],
            }
        }
        5..=12 => {
            let code = match n {
                5 => 15,
                6 => 17,
                7 => 18,
                8 => 19,
                9 => 20,
                10 => 21,
                11 => 23,
                12 => 24,
                _ => unreachable!(),
            };
            csi_tilde(code, mods)
        }
        _ => return None,
    };
    Some(seq)
}
