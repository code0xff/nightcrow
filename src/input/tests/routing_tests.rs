use super::common::{ctrl, key};
use super::*;
use crossterm::event::KeyCode;
/// Every guide a user reads, concatenated at compile time. A missing or moved
/// file is a build error rather than a test that silently stops checking
/// anything — which is what happened when the keybinding tables left the README
/// for a guide of their own and this kept scanning the README alone.
///
/// All of them rather than the keybinding page, because the second direction
/// below only guards what it reads: a `<prefix>` key named in passing anywhere
/// else could be renamed out from under its mention and nothing would say so.
/// `docs/architecture/` and `docs/decisions.md` are deliberately out — they
/// record how the thing was built and what was decided, including keys that no
/// longer exist, which is not drift.
const DOCS: &str = concat!(
    include_str!("../../../README.md"),
    include_str!("../../../docs/getting-started.md"),
    include_str!("../../../docs/projects.md"),
    include_str!("../../../docs/views.md"),
    include_str!("../../../docs/keybindings.md"),
    include_str!("../../../docs/session-state.md"),
    include_str!("../../../docs/web-viewer.md"),
    include_str!("../../../docs/plugins.md"),
    include_str!("../../../docs/configuration.md"),
);

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

/// Every `<prefix> c` the docs spell out, with ranges expanded.
///
/// `<prefix> 3`…`<prefix> 9` documents seven keys, not two — the interior
/// ones never appear on their own, so without expanding them a removed
/// `<prefix> 5` would leave the docs claiming a key that does nothing.
fn documented_prefix_keys() -> Vec<char> {
    let mut found = Vec::new();
    let mut idx = 0;
    while let Some(offset) = DOCS[idx..].find(PREFIX_TOKEN) {
        let start = idx + offset + PREFIX_TOKEN.len();
        idx = start;
        let rest = &DOCS[start..];
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
/// table nobody re-reads. These two tests make the docs answerable to
/// `prefix_action` in both directions.
#[test]
fn every_leader_command_is_documented() {
    let documented = documented_prefix_keys();
    let commands = leader_commands();
    assert!(!commands.is_empty(), "probing found no leader commands");
    for c in commands {
        assert!(
            documented.contains(&c),
            "`<prefix> {c}` works but the docs never mention it"
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
            "the docs document `<prefix> {c}`, which maps to nothing"
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
    assert_eq!(prefix_action(key(KeyCode::Char('u'))), Action::ReloadConfig);
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
