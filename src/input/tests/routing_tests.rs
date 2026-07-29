use super::common::{ctrl, key};
use super::*;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
/// The README, read at compile time. A missing or moved file is a build
/// error rather than a test that silently stops checking anything.
const README: &str = include_str!("../../../README.md");

const PREFIX_TOKEN: &str = "`<prefix> ";

/// Every leader command, derived from `prefix_action` itself by probing
/// the candidate keys. Deriving rather than listing is the point: a
/// hard-coded set cannot notice a command that was just added to the
/// match arm, which is exactly the drift these tests exist to catch.
fn leader_commands() -> Vec<char> {
    ('a'..='z')
        .chain('0'..='9')
        .filter(|&c| {
            let e = key(KeyCode::Char(c));
            prefix_action(e) != Action::None || prefix_action_fullscreen(e) != Action::None
        })
        .collect()
}

/// Every `<prefix> c` the README spells out, with ranges expanded.
///
/// `<prefix> 3`…`<prefix> 9` documents seven keys, not two — the interior
/// ones never appear on their own, so without expanding them a removed
/// `<prefix> 5` would leave the README claiming a key that does nothing.
fn documented_prefix_keys() -> Vec<char> {
    let mut found = Vec::new();
    let mut idx = 0;
    while let Some(offset) = README[idx..].find(PREFIX_TOKEN) {
        let start = idx + offset + PREFIX_TOKEN.len();
        idx = start;
        let rest = &README[start..];
        let mut chars = rest.chars();
        let (Some(c), Some('`')) = (chars.next(), chars.next()) else {
            // `<prefix>` alone, or prose rather than a key.
            continue;
        };
        found.push(c);
        let after = &rest[c.len_utf8() + '`'.len_utf8()..];
        if let Some(tail) = after.strip_prefix('…')
            && let Some(upper) = tail.strip_prefix(PREFIX_TOKEN)
            && let Some(end) = upper.chars().next()
            && c.is_ascii()
            && end.is_ascii()
            && c < end
        {
            found.extend((c as u8 + 1..end as u8).map(char::from));
        }
    }
    found.sort_unstable();
    found.dedup();
    found
}

/// Doc drift is silent: a renamed command leaves the old key sitting in a
/// table nobody re-reads. These two tests make the README answerable to
/// `prefix_action` in both directions.
#[test]
fn every_leader_command_is_documented() {
    let documented = documented_prefix_keys();
    let commands = leader_commands();
    assert!(!commands.is_empty(), "probing found no leader commands");
    for c in commands {
        assert!(
            documented.contains(&c),
            "`<prefix> {c}` works but the README never mentions it"
        );
    }
}

/// Note the limit: a key is accepted if *either* layout maps it, because
/// the parser reads keys out of prose and cannot tell which table row
/// documented them. So a digit dropped from only the split view still
/// passes here. What this does catch is a documented key that does nothing
/// at all — including the interior of a range, thanks to the expansion.
#[test]
fn every_documented_leader_key_still_works() {
    for c in documented_prefix_keys() {
        let e = key(KeyCode::Char(c));
        assert!(
            prefix_action(e) != Action::None || prefix_action_fullscreen(e) != Action::None,
            "the README documents `<prefix> {c}`, which maps to nothing"
        );
    }
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
        prefix_action(key(KeyCode::Char('b'))),
        Action::ToggleTreeView
    );
    assert_eq!(
        prefix_action(key(KeyCode::Char('f'))),
        Action::ToggleFullscreen
    );
    assert_eq!(prefix_action(key(KeyCode::Char('o'))), Action::OpenProject);
    assert_eq!(prefix_action(key(KeyCode::Char('p'))), Action::CycleTheme);
    assert_eq!(prefix_action(key(KeyCode::Char('r'))), Action::Redraw);
    assert_eq!(prefix_action(key(KeyCode::Char('q'))), Action::Quit);
    assert_eq!(
        prefix_action(key(KeyCode::Char('s'))),
        Action::SwapPanePrompt
    );
    assert_eq!(
        prefix_action(key(KeyCode::Char('z'))),
        Action::ClaimPaneSizing
    );
}

#[test]
fn prefix_dispatch_maps_digits_to_focus_and_panes() {
    // Digits mirror the no-prefix F-keys one-for-one: 1=F1 (file list),
    // 2=F2 (diff viewer), 3..9,0=F3..F10 (terminal panes 0..7).
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
    // 0 mirrors F10 (the 8th pane) since digits only go up to 9.
    assert_eq!(
        prefix_action(key(KeyCode::Char('0'))),
        Action::SwitchPane(7)
    );
}

#[test]
fn fullscreen_prefix_maps_digits_one_through_eight_to_panes() {
    // With the upper viewer hidden the whole digit row addresses panes by
    // natural numbering: 1..8 -> panes 0..7.
    for d in 1..=8u8 {
        let c = char::from(b'0' + d);
        assert_eq!(
            prefix_action_fullscreen(key(KeyCode::Char(c))),
            Action::SwitchPane((d - 1) as usize),
            "<prefix> {c} in fullscreen must jump to pane {}",
            d - 1
        );
    }
}

#[test]
fn fullscreen_prefix_drops_nine_and_zero() {
    // Only 8 panes have a jump key, so 9/0 must not fall through to the
    // split-view list/diff/pane bindings.
    assert_eq!(
        prefix_action_fullscreen(key(KeyCode::Char('9'))),
        Action::None
    );
    assert_eq!(
        prefix_action_fullscreen(key(KeyCode::Char('0'))),
        Action::None
    );
}

#[test]
fn fullscreen_prefix_leaves_non_digit_chords_unchanged() {
    // Non-digit chords defer to `prefix_action`, so `f`/`t`/`w`/`s` and the
    // rest keep their meaning while the terminal is fullscreen.
    for (c, expected) in [
        ('f', Action::ToggleFullscreen),
        ('t', Action::NewPane),
        ('w', Action::ClosePane),
        ('s', Action::SwapPanePrompt),
        ('z', Action::ClaimPaneSizing),
        ('q', Action::Quit),
    ] {
        assert_eq!(prefix_action_fullscreen(key(KeyCode::Char(c))), expected);
    }
}

#[test]
fn prefix_dispatch_ignores_modifiers_on_follow_up() {
    // A leftover Ctrl from the leader chord must not break the follow-up.
    assert_eq!(prefix_action(ctrl(KeyCode::Char('t'))), Action::NewPane);
}

#[test]
fn prefix_dispatch_unmapped_key_is_none() {
    assert_eq!(prefix_action(key(KeyCode::Char('v'))), Action::None);
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
