//! The leader chord's follow-up keys.
//!
//! tmux-style: the leader arms, and exactly one key resolves it. Everything here
//! disarms before returning, so an unmapped follow-up costs one keystroke rather
//! than leaving the interface in a mode the user cannot see.

use super::dispatch::{KeyOutcome, handle_global_action};
use crate::app::{App, Focus};
use crate::input::{Action, encode_key, prefix_action, prefix_action_fullscreen};
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

/// Resolve the single key pressed while the prefix is armed. The prefix is
/// always disarmed before returning (tmux-style: one follow-up per leader).
pub(super) fn handle_prefix_followup(app: &mut App, key: KeyEvent) -> KeyOutcome {
    app.cancel_prefix();
    // `<L> <L>`: send the leader chord literally to the focused PTY so the
    // running program still sees the prefix key when the user means it. This
    // is resolved before the Esc/Ctrl+C cancel below so that a `ctrl+c` leader
    // can still deliver a literal Ctrl+C via `<leader><leader>` (Esc remains a
    // universal cancel regardless of the configured leader).
    if app.is_leader_key(key) {
        if app.focus == Focus::Terminal
            && let Some(data) = encode_key(app.leader)
        {
            app.terminal.send_input(&data);
        }
        return KeyOutcome::Continue;
    }

    // Esc / Ctrl+C cancel the prefix without acting. The follow-up key is
    // consumed (not forwarded) so the cancel never leaks into the PTY.
    let is_ctrl_c = key.code == KeyCode::Char('c') && key.modifiers.contains(KeyModifiers::CONTROL);
    if key.code == KeyCode::Esc || is_ctrl_c {
        return KeyOutcome::Continue;
    }

    // A mapped follow-up runs its app command everywhere (terminal + upper).
    let action = resolve_prefix_action(app, key);
    if let Some(outcome) = handle_global_action(app, action) {
        return outcome;
    }
    // Unmapped follow-up: consume and drop it, then return to pass-through.
    KeyOutcome::Continue
}

/// Resolve the key pressed while swap-target mode is armed (`<leader> s`). The
/// mode is always disarmed before returning. A digit that names a pane runs the
/// swap; `Esc`/`Ctrl+C` cancels; any other key is consumed. The digit→pane
/// mapping is reused from `prefix_action` so it matches the focus-jump digits
/// one-for-one (`3`..`9`,`0` → panes `0`..`7`).
pub(super) fn handle_swap_target_followup(app: &mut App, key: KeyEvent) -> KeyOutcome {
    app.cancel_swap_target();

    let is_ctrl_c = key.code == KeyCode::Char('c') && key.modifiers.contains(KeyModifiers::CONTROL);
    if key.code == KeyCode::Esc || is_ctrl_c {
        return KeyOutcome::Continue;
    }
    if let Action::SwitchPane(idx) = resolve_prefix_action(app, key) {
        app.swap_active_pane_with(idx);
    }
    KeyOutcome::Continue
}

/// Pick the leader follow-up mapping for the current layout. While the terminal
/// fills the body the upper viewer is hidden, so `prefix_action_fullscreen`
/// repurposes the digit row `1`..`8` onto panes `0`..`7`; otherwise the normal
/// split-view mapping applies (`1`=list, `2`=diff, `3`..`0`=panes). Shared by
/// the focus-jump and swap-target follow-ups so both stay in lockstep.
pub(super) fn resolve_prefix_action(app: &App, key: KeyEvent) -> Action {
    if app.terminal.fullscreen.fills_body() {
        prefix_action_fullscreen(key)
    } else {
        prefix_action(key)
    }
}
