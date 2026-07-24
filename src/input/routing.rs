use crate::input::Action;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

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
        // dependency), so they own the one jump that has no other single-key
        // route: `F1`..`F10` select project tabs `0`..`9`. Panes and the
        // list/diff focus stay on the leader digits, which are layout-aware
        // (see `prefix_action` / `prefix_action_fullscreen`); project tabs are
        // deliberately NOT layout-aware, so the same F-key reaches the same
        // project in split view and in fullscreen alike.
        KeyCode::F(n @ 1..=10) if no_mods => Action::SwitchProject(n as usize - 1),
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
/// leader chord. The digit row addresses whatever the body is showing: `1` =
/// file list, `2` = diff viewer, `3`..`9`,`0` = terminal panes `0`..`7`. The
/// bare F-keys are a separate axis entirely — they select project tabs — so
/// the two never collide and neither needs to leave room for the other.
pub fn prefix_action(event: KeyEvent) -> Action {
    match event.code {
        KeyCode::Char(c) => match c.to_ascii_lowercase() {
            't' => Action::NewPane,
            'w' => Action::ClosePane,
            'l' => Action::ToggleLogView,
            'b' => Action::ToggleTreeView,
            'f' => Action::ToggleFullscreen,
            's' => Action::SwapPanePrompt,
            'o' => Action::OpenProject,
            'x' => Action::CloseProject,
            'p' => Action::CycleTheme,
            'r' => Action::Redraw,
            'q' => Action::Quit,
            '1' => Action::FocusList,
            '2' => Action::FocusDiff,
            d @ '3'..='9' => Action::SwitchPane(d as usize - '3' as usize),
            '0' => Action::SwitchPane(7),
            _ => Action::None,
        },
        _ => Action::None,
    }
}

/// Leader follow-up mapping used while the terminal fills the body
/// (`TerminalFullscreen::fills_body`). The upper viewer is hidden, so the digit
/// row is repurposed: `1`..`8` address the (up to `MAX_VISIBLE_FULLSCREEN` = 8)
/// terminal panes `0`..`7` by natural numbering instead of the list/diff focus
/// jumps that would only make sense with the viewer on screen. `9`/`0` have no
/// pane in the 8-pane cap and are dropped rather than falling through to the
/// split-view bindings. Every non-digit chord behaves exactly as in
/// `prefix_action`, so `f` (exit fullscreen), `t`, `w`, `s`, etc. are unchanged.
pub fn prefix_action_fullscreen(event: KeyEvent) -> Action {
    if let KeyCode::Char(c @ '0'..='9') = event.code {
        return match c {
            '1'..='8' => Action::SwitchPane(c as usize - '1' as usize),
            _ => Action::None,
        };
    }
    prefix_action(event)
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