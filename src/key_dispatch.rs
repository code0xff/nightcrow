use crate::app::{App, Focus};
use crate::input::{Action, encode_key, map_key, prefix_action, prefix_action_fullscreen};
use crate::key_handlers::{
    handle_empty_key, handle_repo_input_key, handle_terminal_key, handle_upper_key,
};
use crate::workspace::Workspace;
use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers};

#[derive(Debug, PartialEq, Eq)]
pub(crate) enum KeyOutcome {
    Continue,
    /// Force a full repaint on the next frame. Used by the `<prefix> r` redraw
    /// chord to wipe stray glyphs left behind when a PTY child writes cells
    /// ratatui's diff renderer doesn't track.
    Redraw,
    Quit,
    /// The key asked for something only the workspace can do. The handlers
    /// take `&mut App` — one project — so they cannot reach the tab list;
    /// they name the intent here and `main_loop` carries it out.
    Project(ProjectRequest),
}

/// A workspace-level action requested by a key or click.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum ProjectRequest {
    /// Focus the tab at this index. Out-of-range indices are inert.
    Switch(usize),
    /// Close the active tab. Refused when it is the only one.
    Close,
    /// Open this resolved repo path as a tab, or focus the tab already on it.
    Open(String),
    /// Raise the open-repo dialog. It lives on the workspace, so a handler
    /// holding one project cannot open it directly.
    OpenDialog,
}

/// Everything a project needs beyond its repo path.
///
/// Threaded to the input handlers rather than stored on `Workspace` so the
/// workspace stays a pure state container: opening a tab is the only thing
/// that needs the config, and it borrows it for the duration of one keypress.
pub(crate) struct ProjectContext<'a> {
    pub(crate) cfg: &'a crate::config::Config,
    pub(crate) startup_commands: &'a [crate::config::StartupCommand],
    pub(crate) leader: KeyEvent,
}

pub(crate) fn handle_key(app: &mut App, key: KeyEvent) -> KeyOutcome {
    // Crossterm emits Press/Repeat/Release for every keystroke on Windows
    // and on terminals that negotiate the kitty keyboard protocol.
    // Without this guard every keypress would be processed twice or more
    // — visible as doubled search chars, the leader firing repeatedly, and
    // Backspace popping past the buffer.
    if key.kind != KeyEventKind::Press {
        return KeyOutcome::Continue;
    }

    // A key nightcrow acts on itself means the user has moved on, so the
    // notice row goes back to showing repo identity. Keys forwarded verbatim
    // to a PTY are excluded: in a terminal pane every keystroke is
    // passthrough, and dismissing on those would blank a notice the moment
    // the user resumed typing. Runs before dispatch so an action that raises
    // a *new* notice still leaves it standing.
    if app.search_overlay_active()
        || app.prefix_armed()
        || app.awaiting_swap_target()
        || app.is_leader_key(key)
        || app.focus != Focus::Terminal
    {
        app.dismiss_notice_on_app_input();
    }

    // Modal overlays (repo-input dialog, both search bars) own every
    // keystroke until dismissed. They are checked before any leader handling
    // so a leader keypress while a search/repo dialog is open is typed/edited
    // by the overlay rather than arming the prefix.
    if app.search_overlay_active() {
        // A prefix (or swap-target) could only be armed if an overlay opened
        // out from under it; disarm both so neither indicator lingers behind a
        // modal.
        app.cancel_prefix();
        app.cancel_swap_target();
        // Search overlays are handled inside the focus-local upper handler.
        handle_upper_key(app, key, Action::None);
        return KeyOutcome::Continue;
    }

    // Swap-target mode is armed (`<leader> s`): this key is the digit naming
    // the pane to swap the active pane with. Checked before the prefix so its
    // dedicated follow-up handler owns the key.
    if app.awaiting_swap_target() {
        return handle_swap_target_followup(app, key);
    }

    // Prefix is armed: this key is the single follow-up. Resolve it three
    // ways — Esc/Ctrl+C cancels, the leader again sends a literal leader to
    // the PTY, a mapped key runs its action; any other key is consumed.
    if app.prefix_armed() {
        return handle_prefix_followup(app, key);
    }

    // The leader chord arms the prefix; nothing else happens this tick.
    if app.is_leader_key(key) {
        app.arm_prefix();
        return KeyOutcome::Continue;
    }

    let action = map_key(key);
    if let Some(outcome) = handle_global_action(app, action) {
        return outcome;
    }

    match app.focus {
        Focus::Terminal => handle_terminal_key(app, key, action),
        Focus::FileList | Focus::DiffViewer => handle_upper_key(app, key, action),
    }
    KeyOutcome::Continue
}

/// Resolve the single key pressed while the prefix is armed. The prefix is
/// always disarmed before returning (tmux-style: one follow-up per leader).
fn handle_prefix_followup(app: &mut App, key: KeyEvent) -> KeyOutcome {
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
fn handle_swap_target_followup(app: &mut App, key: KeyEvent) -> KeyOutcome {
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
fn resolve_prefix_action(app: &App, key: KeyEvent) -> Action {
    if app.terminal.fullscreen.fills_body() {
        prefix_action_fullscreen(key)
    } else {
        prefix_action(key)
    }
}

fn handle_global_action(app: &mut App, action: Action) -> Option<KeyOutcome> {
    match action {
        Action::Quit => Some(KeyOutcome::Quit),
        Action::NewPane => {
            app.open_new_pane();
            Some(KeyOutcome::Continue)
        }
        Action::ClosePane => {
            // Scoped by `can_close_pane` (terminal focus — the close target
            // is invisible without it). The key is still consumed so it
            // can't leak elsewhere.
            if app.can_close_pane() {
                app.close_active_pane();
            }
            Some(KeyOutcome::Continue)
        }
        // Opening is two steps: this only raises the dialog, and confirming it
        // emits the `Open` request (see `handle_repo_input_key`).
        Action::OpenProject => Some(KeyOutcome::Project(ProjectRequest::OpenDialog)),
        Action::CloseProject => Some(KeyOutcome::Project(ProjectRequest::Close)),
        Action::SwitchProject(idx) => Some(KeyOutcome::Project(ProjectRequest::Switch(idx))),
        Action::ToggleFullscreen => {
            match app.focus {
                Focus::DiffViewer => app.toggle_diff_fullscreen(),
                Focus::FileList => app.toggle_list_fullscreen(),
                Focus::Terminal => app.toggle_terminal_fullscreen(),
            }
            Some(KeyOutcome::Continue)
        }
        Action::ToggleLogView => {
            app.toggle_mode();
            Some(KeyOutcome::Continue)
        }
        Action::ToggleTreeView => {
            app.toggle_tree_mode();
            Some(KeyOutcome::Continue)
        }
        Action::CycleTheme => {
            app.cycle_accent();
            Some(KeyOutcome::Continue)
        }
        Action::Redraw => Some(KeyOutcome::Redraw),
        Action::SwitchPane(n) => {
            app.switch_pane(n);
            Some(KeyOutcome::Continue)
        }
        Action::SwapPanePrompt => {
            // Scoped by `can_swap_panes` (terminal focus plus a second pane).
            // The key is still consumed either way.
            if app.can_swap_panes() {
                app.begin_swap_target();
            }
            Some(KeyOutcome::Continue)
        }
        Action::FocusList => {
            app.focus_list();
            Some(KeyOutcome::Continue)
        }
        Action::FocusDiff => {
            app.focus_diff();
            Some(KeyOutcome::Continue)
        }
        Action::CycleForward => {
            app.cycle_focus_forward();
            Some(KeyOutcome::Continue)
        }
        Action::CycleBackward => {
            app.cycle_focus_backward();
            Some(KeyOutcome::Continue)
        }
        _ => None,
    }
}

pub(crate) fn has_command_modifier(key: KeyEvent) -> bool {
    key.modifiers.intersects(
        KeyModifiers::CONTROL
            | KeyModifiers::ALT
            | KeyModifiers::SUPER
            | KeyModifiers::HYPER
            | KeyModifiers::META,
    )
}

pub(crate) fn text_input_char(key: KeyEvent) -> Option<char> {
    if has_command_modifier(key) {
        return None;
    }
    match key.code {
        KeyCode::Char(c) if !c.is_control() => Some(c),
        _ => None,
    }
}

pub(crate) fn matches_text_command(key: KeyEvent, expected: char) -> bool {
    !has_command_modifier(key) && matches!(key.code, KeyCode::Char(c) if c == expected)
}

/// Route one key, resolving the workspace-level cases first.
///
/// The open dialog and the empty screen both belong to the workspace, and
/// `handle_key` holds a single project, so neither can be dispatched from
/// inside it. Resolving them here keeps `handle_key` — and every test that
/// drives it with one `App` — working on exactly one project.
pub(crate) fn dispatch_key(ws: &mut Workspace, key: KeyEvent) -> KeyOutcome {
    if key.kind != KeyEventKind::Press {
        return KeyOutcome::Continue;
    }
    if ws.repo_input.active {
        return handle_repo_input_key(ws, key);
    }
    match ws.active_mut() {
        Some(app) => handle_key(app, key),
        None => handle_empty_key(ws, key),
    }
}
