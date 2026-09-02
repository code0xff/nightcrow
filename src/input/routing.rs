use crate::input::Action;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

/// Classify a key with NO leader prefix in play. App commands live behind the
/// leader (see `prefix_action`); only modifier-required reserved keys and bare
/// navigation keys remain. The reserved chords are safe global shortcuts
/// because they cannot be confused with prompt text: F-keys are distinct
/// across terminals, and Shift+arrow / Shift+PgUp/PgDn carry a modifier.
pub fn map_key(event: KeyEvent) -> Action {
    // Match reserved chords on their EXACT modifier set so any extra modifier
    // falls through to the PTY: Shift+arrow must be shift-only, and bare
    // F-keys / arrows must carry no modifier at all — including
    // Super/Hyper/Meta, so e.g. Super+F3 passes straight through.
    let shift_only = event.modifiers == KeyModifiers::SHIFT;
    let ctrl_shift = event.modifiers == KeyModifiers::CONTROL | KeyModifiers::SHIFT;
    let no_mods = event.modifiers.is_empty();

    match event.code {
        KeyCode::Left if shift_only => Action::CycleBackward,
        KeyCode::Right if shift_only => Action::CycleForward,
        // One modifier deeper than pane cycling, on the same keys: the arrows
        // step through panes, and adding Ctrl widens the step to project tabs.
        // Exact equality keeps these from swallowing the shift-only arms.
        KeyCode::Left if ctrl_shift => Action::PrevProject,
        KeyCode::Right if ctrl_shift => Action::NextProject,
        KeyCode::Up if shift_only => Action::TermScrollLineUp,
        KeyCode::Down if shift_only => Action::TermScrollLineDown,
        KeyCode::PageUp if shift_only => Action::TermScrollUp,
        KeyCode::PageDown if shift_only => Action::TermScrollDown,
        // F-keys own the one jump with no single-key route: `F1`..`F10` select
        // project tabs `0`..`9`. Project tabs are deliberately NOT layout-aware
        // (unlike the leader digits), so the same F-key reaches the same
        // project in split view and fullscreen alike.
        KeyCode::F(n @ 1..=10) if no_mods => Action::SwitchProject(n as usize - 1),
        KeyCode::Up if no_mods => Action::Up,
        KeyCode::Down if no_mods => Action::Down,
        KeyCode::PageUp if no_mods => Action::PageUp,
        KeyCode::PageDown if no_mods => Action::PageDown,
        // j/k are intentionally NOT mapped here so they remain plain characters
        // when Focus::Terminal forwards them to the PTY. The upper-pane handler
        // interprets them as navigation via `is_vim_navigation_key`.
        _ => Action::None,
    }
}

/// Classify the leader follow-up key. Matched on the bare character
/// regardless of modifiers so `<L> t` works whether or not the user is still
/// holding a modifier from the leader chord. The digit row addresses whatever
/// the body is showing (`1` = file list, `2` = diff viewer, `3`..`9`,`0` =
/// panes `0`..`7`); the bare F-keys are a separate axis (project tabs), so
/// the two never collide. The bracket pair moves the active project tab within
/// the strip, which is why it is here and not in `map_key`.
pub fn prefix_action(event: KeyEvent) -> Action {
    match event.code {
        KeyCode::Char(c) => match c.to_ascii_lowercase() {
            't' => Action::NewPane,
            'w' => Action::ClosePane,
            'l' => Action::ToggleLogView,
            'b' => Action::ToggleTreeView,
            'f' => Action::ToggleFullscreen,
            's' => Action::SwapPanePrompt,
            'z' => Action::ClaimPaneSizing,
            // `c` for cancel. Bare, not `ctrl+c`: the follow-up handler treats
            // `ctrl+c` as the universal prefix cancel.
            'c' => Action::CancelRecovery,
            'o' => Action::OpenProject,
            'x' => Action::CloseProject,
            // Behind the leader, never bare: `map_key` passes unmodified
            // characters straight to the PTY, so intercepting `[`/`]` there
            // would break terminal input and escape sequences.
            '[' => Action::MoveProjectPrev,
            ']' => Action::MoveProjectNext,
            'p' => Action::CycleTheme,
            // `u` for update-from-file. Not `r`, which is already Redraw.
            'u' => Action::ReloadConfig,
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

/// Leader follow-up mapping while the terminal fills the body
/// (`TerminalFullscreen::fills_body`): the upper viewer is hidden, so the
/// digit row is repurposed onto panes `0`..`7` by natural numbering instead
/// of the list/diff focus jumps. `9`/`0` address no pane within the 8-pane
/// cap and are dropped rather than falling through to the split-view
/// bindings. Every non-digit chord behaves as in `prefix_action`.
pub fn prefix_action_fullscreen(event: KeyEvent) -> Action {
    if let KeyCode::Char(c @ '0'..='9') = event.code {
        return match c {
            '1'..='8' => Action::SwitchPane(c as usize - '1' as usize),
            _ => Action::None,
        };
    }
    prefix_action(event)
}

/// `Some(Action::Up | Action::Down)` for bare vim-style j/k, else `None`.
/// Used by upper-pane handlers so terminal pass-through is unaffected by
/// changes in `map_key`.
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
