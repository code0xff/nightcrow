//! App-level wrappers around `TerminalState` mixing in cross-cutting state
//! (focus, status line, fullscreen flags). Pure terminal logic lives on
//! `TerminalState` in `runtime/terminal.rs`.

use super::{App, Focus, NoticeKind};
use crate::runtime::terminal::TerminalFullscreen;

impl App {
    pub fn poll_terminal(&mut self) {
        // `TerminalState::poll` only signals exited panes; re-clamping focus
        // and fullscreen when the active pane was one of them stays here.
        if !self.terminal.poll().is_empty() {
            self.clamp_active_pane_after_removal();
        }
        // The panes arrived, so the terminal half of the session — which pane
        // was active, whether the panel was fullscreen, whether the input focus
        // was there — can finally be applied. It waits here rather than at
        // construction because none of it means anything against an empty pane
        // list (see `pending_terminal`).
        if !self.terminal.panes.is_empty()
            && let Some(state) = self.pending_terminal.take()
        {
            self.restore_pane_focus(&state);
        }
    }

    pub fn open_new_pane(&mut self) {
        if let Err(e) = self.terminal.create_pane() {
            tracing::error!("create_terminal_pane failed: {e}");
            self.raise_notice(NoticeKind::Terminal, e.to_string());
            return;
        }
        self.clear_notice(NoticeKind::Terminal);
        // `create_pane` made the new pane active; move app focus onto it and
        // drop competing fullscreen so focus/render/hints stay in sync.
        self.focus = Focus::Terminal;
        self.diff.fullscreen = false;
        self.list_fullscreen = false;
    }

    pub fn close_active_pane(&mut self) {
        if self.terminal.close_active() {
            self.clamp_active_pane_after_removal();
        }
    }

    // Without terminal focus the active pane is rendered indistinguishable
    // from the others, so the close target would be invisible. Single source
    // for the key gate and the hint rows so they can never disagree.
    pub fn can_close_pane(&self) -> bool {
        self.focus == Focus::Terminal
    }

    // close's terminal-focus scope plus a second pane to swap with — fewer
    // and every target digit would be a no-op. Single source like `can_close_pane`.
    pub fn can_swap_panes(&self) -> bool {
        self.focus == Focus::Terminal && self.terminal.panes.len() > 1
    }

    pub(crate) fn clamp_active_pane_after_removal(&mut self) {
        if self.terminal.panes.is_empty() {
            self.terminal.active = 0;
            self.terminal.fullscreen = TerminalFullscreen::Off;
            // Only redirect focus when it was on the terminal — otherwise an
            // externally-exited last pane would yank focus from where the user
            // was working.
            if self.focus == Focus::Terminal {
                self.focus = Focus::DiffViewer;
            }
        } else {
            self.terminal.active = self.terminal.active.min(self.terminal.panes.len() - 1);
            // Normalize a `Zoom` that's now indistinguishable from `Grid` so
            // the held state matches the invariant the cycle keeps.
            if self.terminal.fullscreen == TerminalFullscreen::Zoom
                && !self.terminal.zoom_distinct_from_grid()
            {
                self.terminal.fullscreen = TerminalFullscreen::Grid;
            }
        }
        self.terminal.sync_visible_window();
    }

    pub fn switch_pane(&mut self, idx: usize) {
        if idx < self.terminal.panes.len() {
            self.terminal.active = idx;
            self.terminal.sync_visible_window();
            self.focus = Focus::Terminal;
            // Drop competing fullscreen so focus/render/hints stay in sync.
            self.diff.fullscreen = false;
            self.list_fullscreen = false;
        }
    }

    pub fn swap_active_pane_with(&mut self, idx: usize) {
        if self.terminal.swap_active_with(idx) {
            self.focus = Focus::Terminal;
            self.diff.fullscreen = false;
            self.list_fullscreen = false;
        }
    }

    pub fn active_screen(&self) -> Option<crate::runtime::emulator::ScreenView<'_>> {
        self.terminal.active_screen()
    }
}
