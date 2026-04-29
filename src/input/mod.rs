use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

#[derive(Debug, Clone, PartialEq)]
pub enum Action {
    Quit,
    Up,
    Down,
    PageUp,
    PageDown,
    FocusNext,
    FocusPrev,
    UpperFocusToggle,
    NewPane,
    SwitchPane(usize),
    None,
}

pub fn map_key(event: KeyEvent) -> Action {
    let ctrl = event.modifiers.contains(KeyModifiers::CONTROL);

    match event.code {
        KeyCode::Char('q') if ctrl => Action::Quit,
        KeyCode::Char('t') if ctrl => Action::NewPane,
        KeyCode::F(n @ 1..=9) => Action::SwitchPane(n as usize - 1),
        KeyCode::Left | KeyCode::Right => Action::UpperFocusToggle,
        KeyCode::Tab => Action::FocusNext,
        KeyCode::BackTab => Action::FocusPrev,
        KeyCode::Up | KeyCode::Char('k') => Action::Up,
        KeyCode::Down | KeyCode::Char('j') => Action::Down,
        KeyCode::PageUp => Action::PageUp,
        KeyCode::PageDown => Action::PageDown,
        _ => Action::None,
    }
}

/// Encode a crossterm KeyEvent as VT100/ANSI bytes for terminal pass-through.
pub fn encode_key(key: KeyEvent) -> Option<Vec<u8>> {
    let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
    let alt = key.modifiers.contains(KeyModifiers::ALT);

    match key.code {
        KeyCode::Char(c) => {
            if ctrl {
                let b = (c.to_ascii_uppercase() as u8).wrapping_sub(b'@');
                if b < 32 {
                    return Some(vec![b]);
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
        KeyCode::Enter => Some(vec![b'\r']),
        KeyCode::Backspace => Some(vec![0x7f]),
        KeyCode::Delete => Some(b"\x1b[3~".to_vec()),
        KeyCode::Esc => Some(vec![0x1b]),
        KeyCode::Tab => Some(vec![b'\t']),
        KeyCode::BackTab => Some(b"\x1b[Z".to_vec()),
        KeyCode::Up => Some(b"\x1b[A".to_vec()),
        KeyCode::Down => Some(b"\x1b[B".to_vec()),
        KeyCode::Right => Some(b"\x1b[C".to_vec()),
        KeyCode::Left => Some(b"\x1b[D".to_vec()),
        KeyCode::Home => Some(b"\x1b[H".to_vec()),
        KeyCode::End => Some(b"\x1b[F".to_vec()),
        KeyCode::PageUp => Some(b"\x1b[5~".to_vec()),
        KeyCode::PageDown => Some(b"\x1b[6~".to_vec()),
        KeyCode::F(n) => Some(match n {
            1 => b"\x1bOP".to_vec(),
            2 => b"\x1bOQ".to_vec(),
            3 => b"\x1bOR".to_vec(),
            4 => b"\x1bOS".to_vec(),
            5 => b"\x1b[15~".to_vec(),
            6 => b"\x1b[17~".to_vec(),
            7 => b"\x1b[18~".to_vec(),
            8 => b"\x1b[19~".to_vec(),
            9 => b"\x1b[20~".to_vec(),
            10 => b"\x1b[21~".to_vec(),
            11 => b"\x1b[23~".to_vec(),
            12 => b"\x1b[24~".to_vec(),
            _ => return None,
        }),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::NONE)
    }

    fn ctrl(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::CONTROL)
    }

    #[test]
    fn maps_quit_shortcuts() {
        assert_eq!(map_key(ctrl(KeyCode::Char('q'))), Action::Quit);
        // Plain 'q' must pass through (terminal apps like less/vim use it).
        assert_ne!(map_key(key(KeyCode::Char('q'))), Action::Quit);
        // Ctrl+C is a terminal signal, not a quit shortcut.
        assert_ne!(map_key(ctrl(KeyCode::Char('c'))), Action::Quit);
    }

    #[test]
    fn maps_navigation_shortcuts() {
        assert_eq!(map_key(key(KeyCode::Up)), Action::Up);
        assert_eq!(map_key(key(KeyCode::Char('k'))), Action::Up);
        assert_eq!(map_key(key(KeyCode::Down)), Action::Down);
        assert_eq!(map_key(key(KeyCode::Char('j'))), Action::Down);
        assert_eq!(map_key(key(KeyCode::PageUp)), Action::PageUp);
        assert_eq!(map_key(key(KeyCode::PageDown)), Action::PageDown);
    }

    #[test]
    fn maps_focus_cycle() {
        assert_eq!(map_key(key(KeyCode::Tab)), Action::FocusNext);
        assert_eq!(map_key(key(KeyCode::BackTab)), Action::FocusPrev);
    }

    #[test]
    fn maps_upper_focus_toggle() {
        assert_eq!(map_key(key(KeyCode::Left)), Action::UpperFocusToggle);
        assert_eq!(map_key(key(KeyCode::Right)), Action::UpperFocusToggle);
    }

    #[test]
    fn maps_new_pane() {
        assert_eq!(map_key(ctrl(KeyCode::Char('t'))), Action::NewPane);
    }

    #[test]
    fn maps_switch_pane() {
        assert_eq!(map_key(key(KeyCode::F(1))), Action::SwitchPane(0));
        assert_eq!(map_key(key(KeyCode::F(2))), Action::SwitchPane(1));
        assert_eq!(map_key(key(KeyCode::F(9))), Action::SwitchPane(8));
    }

    #[test]
    fn encode_printable_char() {
        assert_eq!(encode_key(key(KeyCode::Char('a'))), Some(b"a".to_vec()));
    }

    #[test]
    fn encode_ctrl_c_as_etx() {
        assert_eq!(encode_key(ctrl(KeyCode::Char('c'))), Some(vec![0x03]));
    }

    #[test]
    fn encode_arrow_keys() {
        assert_eq!(encode_key(key(KeyCode::Up)), Some(b"\x1b[A".to_vec()));
        assert_eq!(encode_key(key(KeyCode::Down)), Some(b"\x1b[B".to_vec()));
    }

    #[test]
    fn encode_enter_as_cr() {
        assert_eq!(encode_key(key(KeyCode::Enter)), Some(vec![b'\r']));
    }
}
