use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    Quit,
    Up,
    Down,
    PageUp,
    PageDown,
    NewPane,
    ClosePane,
    ChangeRepo,
    ToggleFullscreen,
    SwitchPane(usize),
    FocusList,
    FocusDiff,
    CycleForward,
    CycleBackward,
    TermScrollUp,
    TermScrollDown,
    TermScrollLineUp,
    TermScrollLineDown,
    ToggleLogView,
    CycleTheme,
    Redraw,
    None,
}

/// Classify a key with NO leader prefix in play. App commands are no longer
/// reachable here (they moved behind the leader — see `prefix_action`); only
/// the modifier-required reserved keys and the bare navigation keys remain.
///
/// Reserved no-prefix keys are safe global shortcuts because they cannot be
/// confused with prompt text: F-keys are distinct across terminals, and the
/// Shift+arrow / Shift+PgUp/PgDn chords carry a modifier.
pub fn map_key(event: KeyEvent) -> Action {
    // Match reserved chords on their EXACT modifier set so any extra modifier
    // falls through to the PTY: Shift+arrow must be shift-only (not
    // Ctrl+Shift+arrow), and the bare F-keys / arrows must carry no modifier at
    // all — including Super/Hyper/Meta, so e.g. Super+F3 passes straight
    // through instead of triggering a focus jump.
    let shift_only = event.modifiers == KeyModifiers::SHIFT;
    let no_mods = event.modifiers.is_empty();

    match event.code {
        KeyCode::Left if shift_only => Action::CycleBackward,
        KeyCode::Right if shift_only => Action::CycleForward,
        KeyCode::Up if shift_only => Action::TermScrollLineUp,
        KeyCode::Down if shift_only => Action::TermScrollLineDown,
        KeyCode::PageUp if shift_only => Action::TermScrollUp,
        KeyCode::PageDown if shift_only => Action::TermScrollDown,
        // F-keys are universally distinct across terminals (no kitty protocol
        // dependency), so they own focus jumps: F1=list, F2=diff,
        // F3..=F9 = terminal panes 1..=7.
        KeyCode::F(1) if no_mods => Action::FocusList,
        KeyCode::F(2) if no_mods => Action::FocusDiff,
        KeyCode::F(n @ 3..=9) if no_mods => Action::SwitchPane(n as usize - 3),
        KeyCode::Up if no_mods => Action::Up,
        KeyCode::Down if no_mods => Action::Down,
        KeyCode::PageUp if no_mods => Action::PageUp,
        KeyCode::PageDown if no_mods => Action::PageDown,
        // j/k are intentionally NOT mapped here so they remain plain
        // characters when Focus::Terminal forwards them to the PTY. The
        // upper-pane handler interprets them as navigation explicitly via
        // `is_vim_navigation_key`.
        _ => Action::None,
    }
}

/// Classify the single follow-up key pressed after the leader. Returns the
/// app `Action` the leader chord maps to, or `Action::None` for an unmapped
/// follow-up (which the dispatcher consumes and drops).
///
/// The follow-up is matched on the bare character regardless of modifiers so
/// `<L> t` works whether or not the user is still holding a modifier from the
/// leader chord. Digits mirror the no-prefix focus/pane F-keys one-for-one:
/// `1` = `F1` (file list), `2` = `F2` (diff viewer), `3`..`9` = `F3`..`F9`
/// (terminal panes `0`..`6`).
pub fn prefix_action(event: KeyEvent) -> Action {
    match event.code {
        KeyCode::Char(c) => match c.to_ascii_lowercase() {
            't' => Action::NewPane,
            'w' => Action::ClosePane,
            'l' => Action::ToggleLogView,
            'f' => Action::ToggleFullscreen,
            'o' => Action::ChangeRepo,
            'p' => Action::CycleTheme,
            'r' => Action::Redraw,
            'q' => Action::Quit,
            '1' => Action::FocusList,
            '2' => Action::FocusDiff,
            d @ '3'..='9' => Action::SwitchPane(d as usize - '3' as usize),
            _ => Action::None,
        },
        _ => Action::None,
    }
}

/// Returns `Some(Action::Up | Action::Down)` for the vim-style j/k navigation
/// keys (no modifiers), and `None` otherwise. Used by upper-pane handlers so
/// that terminal pass-through is unaffected by changes in `map_key`.
pub fn vim_navigation_action(key: KeyEvent) -> Option<Action> {
    if !key.modifiers.is_empty() {
        return None;
    }
    match key.code {
        KeyCode::Char('k') => Some(Action::Up),
        KeyCode::Char('j') => Some(Action::Down),
        _ => None,
    }
}

/// Encode a crossterm KeyEvent as VT100/ANSI bytes for terminal pass-through.
pub fn encode_key(key: KeyEvent) -> Option<Vec<u8>> {
    let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
    let alt = key.modifiers.contains(KeyModifiers::ALT);

    match key.code {
        KeyCode::Char(c) => {
            if ctrl && c.is_ascii() {
                // Several Ctrl chords fall outside the contiguous
                // `c.to_ascii_uppercase() - '@' < 32` range and need
                // explicit xterm-convention mappings:
                //   Ctrl+Space → NUL (formula wraps because ' ' < '@')
                //   Ctrl+/     → 0x1F (US): screen/tmux/emacs/less use this
                //   Ctrl+?     → 0x7F (DEL): xterm convention
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
                    // readline / Emacs expectations). Without the prefix
                    // programs like Emacs would see plain Ctrl+Char.
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
        KeyCode::Enter => Some(vec![b'\r']),
        KeyCode::Backspace => Some(vec![0x7f]),
        KeyCode::Delete => Some(csi_tilde(3, key.modifiers)),
        KeyCode::Esc => Some(vec![0x1b]),
        KeyCode::Tab => Some(vec![b'\t']),
        KeyCode::BackTab => Some(b"\x1b[Z".to_vec()),
        KeyCode::Up => Some(csi_cursor(b'A', key.modifiers)),
        KeyCode::Down => Some(csi_cursor(b'B', key.modifiers)),
        KeyCode::Right => Some(csi_cursor(b'C', key.modifiers)),
        KeyCode::Left => Some(csi_cursor(b'D', key.modifiers)),
        KeyCode::Home => Some(csi_cursor(b'H', key.modifiers)),
        KeyCode::End => Some(csi_cursor(b'F', key.modifiers)),
        KeyCode::PageUp => Some(csi_tilde(5, key.modifiers)),
        KeyCode::PageDown => Some(csi_tilde(6, key.modifiers)),
        KeyCode::F(n) => encode_function_key(n, key.modifiers),
        _ => None,
    }
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
    fn single_ctrl_keys_are_no_longer_app_commands() {
        // The leader redesign removed bare Ctrl app shortcuts: these now pass
        // through to the PTY (Action::None) so the running program receives
        // them as control bytes.
        for c in ['q', 't', 'w', 'o', 'f', 'l', 'p'] {
            assert_eq!(
                map_key(ctrl(KeyCode::Char(c))),
                Action::None,
                "ctrl+{c} must no longer be a no-prefix app command"
            );
        }
        // Plain 'q' must pass through (terminal apps like less/vim use it).
        assert_ne!(map_key(key(KeyCode::Char('q'))), Action::Quit);
    }

    #[test]
    fn prefix_dispatch_maps_app_commands() {
        assert_eq!(prefix_action(key(KeyCode::Char('t'))), Action::NewPane);
        assert_eq!(prefix_action(key(KeyCode::Char('w'))), Action::ClosePane);
        assert_eq!(
            prefix_action(key(KeyCode::Char('l'))),
            Action::ToggleLogView
        );
        assert_eq!(
            prefix_action(key(KeyCode::Char('f'))),
            Action::ToggleFullscreen
        );
        assert_eq!(prefix_action(key(KeyCode::Char('o'))), Action::ChangeRepo);
        assert_eq!(prefix_action(key(KeyCode::Char('p'))), Action::CycleTheme);
        assert_eq!(prefix_action(key(KeyCode::Char('r'))), Action::Redraw);
        assert_eq!(prefix_action(key(KeyCode::Char('q'))), Action::Quit);
    }

    #[test]
    fn prefix_dispatch_maps_digits_to_focus_and_panes() {
        // Digits mirror the no-prefix F-keys one-for-one: 1=F1 (file list),
        // 2=F2 (diff viewer), 3..9=F3..F9 (terminal panes 0..6).
        assert_eq!(prefix_action(key(KeyCode::Char('1'))), Action::FocusList);
        assert_eq!(prefix_action(key(KeyCode::Char('2'))), Action::FocusDiff);
        assert_eq!(
            prefix_action(key(KeyCode::Char('3'))),
            Action::SwitchPane(0)
        );
        assert_eq!(
            prefix_action(key(KeyCode::Char('9'))),
            Action::SwitchPane(6)
        );
        // 0 has no F-key counterpart and falls through to None.
        assert_eq!(prefix_action(key(KeyCode::Char('0'))), Action::None);
    }

    #[test]
    fn prefix_dispatch_ignores_modifiers_on_follow_up() {
        // A leftover Ctrl from the leader chord must not break the follow-up.
        assert_eq!(prefix_action(ctrl(KeyCode::Char('t'))), Action::NewPane);
    }

    #[test]
    fn prefix_dispatch_unmapped_key_is_none() {
        assert_eq!(prefix_action(key(KeyCode::Char('z'))), Action::None);
        assert_eq!(prefix_action(key(KeyCode::Esc)), Action::None);
    }

    #[test]
    fn maps_navigation_shortcuts() {
        assert_eq!(map_key(key(KeyCode::Up)), Action::Up);
        assert_eq!(map_key(key(KeyCode::Down)), Action::Down);
        assert_eq!(map_key(key(KeyCode::PageUp)), Action::PageUp);
        assert_eq!(map_key(key(KeyCode::PageDown)), Action::PageDown);
        // j/k are no longer remapped to Up/Down by map_key — they must
        // pass through as Action::None so terminal focus can forward them
        // verbatim to the PTY.
        assert_eq!(map_key(key(KeyCode::Char('k'))), Action::None);
        assert_eq!(map_key(key(KeyCode::Char('j'))), Action::None);
    }

    #[test]
    fn reserved_keys_require_exact_modifiers() {
        use KeyModifiers as M;
        let with = |code, mods| map_key(KeyEvent::new(code, mods));

        // Shift-only arrows are reserved.
        assert_eq!(with(KeyCode::Left, M::SHIFT), Action::CycleBackward);
        // Extra modifiers fall through to the PTY.
        assert_eq!(with(KeyCode::Left, M::SHIFT | M::CONTROL), Action::None);
        assert_eq!(with(KeyCode::Right, M::SHIFT | M::ALT), Action::None);
        // F-keys are reserved only without modifiers.
        assert_eq!(with(KeyCode::F(3), M::NONE), Action::SwitchPane(0));
        assert_eq!(with(KeyCode::F(3), M::ALT), Action::None);
        assert_eq!(with(KeyCode::F(1), M::CONTROL), Action::None);
        // Bare navigation keys with a modifier pass through too.
        assert_eq!(with(KeyCode::Up, M::CONTROL), Action::None);
        assert_eq!(with(KeyCode::Up, M::NONE), Action::Up);
        // Super/Hyper/Meta count as modifiers and must not be ignored.
        assert_eq!(with(KeyCode::F(3), M::SUPER), Action::None);
        assert_eq!(with(KeyCode::Left, M::SHIFT | M::SUPER), Action::None);
    }

    #[test]
    fn encode_key_emits_xterm_modifier_sequences() {
        use KeyModifiers as M;
        let enc = |code, mods| encode_key(KeyEvent::new(code, mods)).unwrap();

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
    fn vim_navigation_for_j_k() {
        assert_eq!(
            vim_navigation_action(key(KeyCode::Char('k'))),
            Some(Action::Up)
        );
        assert_eq!(
            vim_navigation_action(key(KeyCode::Char('j'))),
            Some(Action::Down)
        );
        // Modifiers must disable the vim mapping (e.g. Ctrl-J / Shift-K).
        assert_eq!(vim_navigation_action(ctrl(KeyCode::Char('j'))), None);
        assert_eq!(vim_navigation_action(key(KeyCode::Char('h'))), None);
    }

    #[test]
    fn maps_cycle_pane_shortcuts() {
        let shift_right = KeyEvent::new(KeyCode::Right, KeyModifiers::SHIFT);
        let shift_left = KeyEvent::new(KeyCode::Left, KeyModifiers::SHIFT);
        assert_eq!(map_key(shift_right), Action::CycleForward);
        assert_eq!(map_key(shift_left), Action::CycleBackward);
    }

    #[test]
    fn maps_terminal_scroll_shortcuts() {
        let shift_pgup = KeyEvent::new(KeyCode::PageUp, KeyModifiers::SHIFT);
        let shift_pgdn = KeyEvent::new(KeyCode::PageDown, KeyModifiers::SHIFT);
        let shift_up = KeyEvent::new(KeyCode::Up, KeyModifiers::SHIFT);
        let shift_down = KeyEvent::new(KeyCode::Down, KeyModifiers::SHIFT);
        assert_eq!(map_key(shift_pgup), Action::TermScrollUp);
        assert_eq!(map_key(shift_pgdn), Action::TermScrollDown);
        assert_eq!(map_key(shift_up), Action::TermScrollLineUp);
        assert_eq!(map_key(shift_down), Action::TermScrollLineDown);
        // Plain up/down must not trigger terminal scroll.
        assert_ne!(map_key(key(KeyCode::Up)), Action::TermScrollLineUp);
        assert_ne!(map_key(key(KeyCode::Down)), Action::TermScrollLineDown);
    }

    #[test]
    fn maps_focus_jump_shortcuts() {
        assert_eq!(map_key(key(KeyCode::F(1))), Action::FocusList);
        assert_eq!(map_key(key(KeyCode::F(2))), Action::FocusDiff);
    }

    #[test]
    fn maps_switch_pane() {
        // F3..=F9 directly select terminal panes 0..=6.
        assert_eq!(map_key(key(KeyCode::F(3))), Action::SwitchPane(0));
        assert_eq!(map_key(key(KeyCode::F(4))), Action::SwitchPane(1));
        assert_eq!(map_key(key(KeyCode::F(9))), Action::SwitchPane(6));
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
    fn encode_ctrl_non_ascii_does_not_truncate_to_control_byte() {
        assert_eq!(
            encode_key(ctrl(KeyCode::Char('ŀ'))),
            Some("ŀ".as_bytes().to_vec())
        );
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

    #[test]
    fn encode_ctrl_space_as_nul() {
        // xterm convention: Ctrl+Space → NUL. The generic `c - '@'` formula
        // wraps for space (0x20 < 0x40), so this case needs special handling.
        assert_eq!(encode_key(ctrl(KeyCode::Char(' '))), Some(vec![0x00]));
    }

    #[test]
    fn encode_ctrl_slash_as_us() {
        // Ctrl+/ is conventionally 0x1F (US) on xterm; vim/less/emacs
        // bindings depend on it. Without the explicit mapping the slash
        // fell through as a literal '/' character.
        assert_eq!(encode_key(ctrl(KeyCode::Char('/'))), Some(vec![0x1F]));
    }

    #[test]
    fn encode_ctrl_question_as_del() {
        // Ctrl+? is conventionally DEL (0x7F).
        assert_eq!(encode_key(ctrl(KeyCode::Char('?'))), Some(vec![0x7F]));
    }

    #[test]
    fn encode_ctrl_right_bracket_via_formula() {
        // Sanity check: the `c.to_ascii_uppercase() - '@'` formula already
        // covered Ctrl+]. Pin it so a future refactor of the special-case
        // table doesn't accidentally regress it.
        assert_eq!(encode_key(ctrl(KeyCode::Char(']'))), Some(vec![0x1D]));
    }

    #[test]
    fn encode_ctrl_alt_char_prefixes_esc_to_control_byte() {
        // readline / Emacs convention: Ctrl+Alt+Char → ESC + control byte.
        let key = KeyEvent::new(
            KeyCode::Char('c'),
            KeyModifiers::CONTROL | KeyModifiers::ALT,
        );
        assert_eq!(encode_key(key), Some(vec![0x1b, 0x03]));
    }
}
