//! The no-prefix key map: navigation, reserved chords, and the F-key jumps.
//!
//! Split from the leader mapping, which is a separate table with a separate
//! rule — a follow-up ignores modifiers, while these must match them exactly or
//! a chord meant for the pane below becomes an app command.

use super::common::{ctrl, key};
use super::*;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

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
    assert_eq!(with(KeyCode::F(3), M::NONE), Action::SwitchProject(2));
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
fn f_keys_select_project_tabs_regardless_of_layout() {
    // F1..=F10 select project tabs 0..=9 — the whole row, with no gap for
    // list/diff focus, which lives on the leader digits instead. Panes and
    // list/diff focus are layout-aware (see `prefix_action_fullscreen`), but
    // project tabs deliberately are not: there is one mapping, so the same
    // F-key reaches the same project whether or not the terminal fills the
    // body.
    for n in 1..=10u8 {
        assert_eq!(
            map_key(key(KeyCode::F(n))),
            Action::SwitchProject((n - 1) as usize),
            "F{n} must select project tab {}",
            n - 1
        );
    }
    assert_eq!(map_key(key(KeyCode::F(1))), Action::SwitchProject(0));
    assert_eq!(map_key(key(KeyCode::F(8))), Action::SwitchProject(7));
}
