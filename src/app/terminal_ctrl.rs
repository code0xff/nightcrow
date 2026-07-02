//! App-level wrappers around `TerminalState` that mix in cross-cutting state
//! (focus, status line, fullscreen flags). The pure terminal logic — event
//! drain, pane create/close, vt100 parser bookkeeping — lives on
//! `TerminalState` in `runtime/terminal.rs`; this module exists for the
//! handful of actions that have to coordinate that logic with the rest of
//! `App`'s state machine.

use super::{App, Focus};

impl App {
    /// Open the panes nightcrow starts with. With no reserved
    /// `startup_commands`, this keeps the historical single empty-shell
    /// behaviour. Otherwise it opens one pane per command, runs each command
    /// immediately, and labels the tab with the command's `name` (falling
    /// back to the command text).
    ///
    /// On a fresh launch the input focus lands on the first pane so a cockpit
    /// user can type into the startup program (or the empty shell) right away.
    /// A restored session overrides this later in `restore_session`, so the
    /// last active pane/focus still wins on restart. When no pane could be
    /// created (no backend, or `create_pane` failed), focus stays on the file
    /// list — there is no terminal to focus.
    pub(crate) fn ensure_initial_terminal(
        &mut self,
        startup_commands: &[crate::config::StartupCommand],
    ) {
        if self.terminal.backend.is_none() {
            return;
        }
        if startup_commands.is_empty() {
            if let Err(err) = self.terminal.create_pane() {
                self.status = Some(format!("terminal error: {err}"));
            }
        } else {
            for sc in startup_commands {
                let label = sc.name.as_deref();
                if let Err(err) = self.terminal.create_pane_with(Some(&sc.command), label) {
                    self.status = Some(format!("terminal error: {err}"));
                }
            }
        }
        // Start at the top of the reserved set rather than the last pane
        // created, and put input focus on it so keystrokes reach the terminal
        // program immediately on first launch.
        if !self.terminal.panes.is_empty() {
            self.terminal.active = 0;
            self.terminal.sync_visible_window();
            self.focus = Focus::Terminal;
        }
    }

    pub fn poll_terminal(&mut self) {
        // `TerminalState::poll` only signals which panes the backend exited;
        // re-clamping focus and fullscreen when one of them was the active
        // pane is cross-cutting and stays here.
        if !self.terminal.poll().is_empty() {
            self.clamp_active_pane_after_removal();
        }
    }

    pub fn open_new_pane(&mut self) {
        if let Err(e) = self.terminal.create_pane() {
            tracing::error!("create_terminal_pane failed: {e}");
            self.status = Some(format!("terminal error: {e}"));
            return;
        }
        // `create_pane` already made the new pane the active one within
        // `TerminalState`; move the app-level focus onto it too so the user
        // lands in the freshly opened terminal instead of staying on the
        // file list or diff viewer. Drop competing fullscreen flags for the
        // same reason `switch_pane` does — keep focus, render, and hints in sync.
        self.focus = Focus::Terminal;
        self.diff.fullscreen = false;
        self.list_fullscreen = false;
    }

    pub fn close_active_pane(&mut self) {
        if self.terminal.close_active() {
            self.clamp_active_pane_after_removal();
        }
    }

    pub(crate) fn clamp_active_pane_after_removal(&mut self) {
        if self.terminal.panes.is_empty() {
            self.terminal.active = 0;
            self.terminal.fullscreen = false;
            // Only redirect focus when it was actually on the terminal —
            // otherwise an externally-exited last pane (Ctrl+D in the only
            // shell while the user was reading the diff) would yank focus
            // away from where the user was working.
            if self.focus == Focus::Terminal {
                self.focus = Focus::DiffViewer;
            }
        } else {
            self.terminal.active = self.terminal.active.min(self.terminal.panes.len() - 1);
        }
        // The pane count (and possibly `active`) just changed — re-clamp the
        // split-view window so it still points at real panes.
        self.terminal.sync_visible_window();
    }

    /// Move terminal focus to pane `idx`. This is a focus jump, not a tab
    /// switch: every visible pane keeps rendering, this only changes which
    /// one is active (cursor, input, and the accent-bordered cell).
    pub fn switch_pane(&mut self, idx: usize) {
        if idx < self.terminal.panes.len() {
            self.terminal.active = idx;
            self.terminal.sync_visible_window();
            self.focus = Focus::Terminal;
            // Pressing F1..=F9 is a request to interact with a terminal pane;
            // drop any competing fullscreen so focus, render, and hints stay
            // in sync (otherwise a zoomed diff/list would persist while focus
            // moves away).
            self.diff.fullscreen = false;
            self.list_fullscreen = false;
        }
    }

    pub fn active_screen(&self) -> Option<&vt100::Screen> {
        self.terminal.active_screen()
    }
}
