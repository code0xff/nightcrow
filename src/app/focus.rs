use super::{App, Focus, ViewMode};

impl App {
    pub fn toggle_mode(&mut self) {
        self.clear_diff_state();
        let from = self.mode;
        // Terminal/diff fullscreen hides the list pane, so a mode toggle there
        // would flip state invisibly behind the zoomed pane. Reveal the result
        // with the same policy as `focus_list` (F1). `list_fullscreen` is not
        // part of this check: it already renders the mode's active list, so the
        // swap is visible and the zoom should survive the toggle.
        let reveal_after_toggle = self.terminal.fullscreen || self.diff.fullscreen;
        match self.mode {
            // `<prefix> l` from either Status or Tree enters the Log view.
            ViewMode::Status | ViewMode::Tree => self.enter_log_mode(),
            ViewMode::Log => {
                self.mode = ViewMode::Status;
                self.log_view.reset_drill_down();
                self.refresh_diff(true);
            }
        }
        if reveal_after_toggle {
            self.focus_list();
        }
        tracing::debug!(from = ?from, to = ?self.mode, "view mode toggled");
    }

    /// Switch into the Log view from the current mode. Shared by `<prefix> l`
    /// from both Status and Tree. Reuses cached commit pages when they still
    /// match the latest observed HEAD; otherwise refreshes in the background.
    fn enter_log_mode(&mut self) {
        self.mode = ViewMode::Log;
        self.log_view.reset_drill_down();
        self.log_view.commit_scroll_x = 0;
        // Reuse cached pages on re-entry only while they still match
        // the latest HEAD observed by the snapshot worker. Status mode
        // intentionally does not refresh the hidden commit list, so a
        // HEAD change there must invalidate the cache on the next entry.
        let cached_head = self.log_view.commits.first().map(|c| c.oid);
        let cache_matches_head =
            !self.log_view.commits.is_empty() && cached_head == self.pagination.last_head_oid;
        if !self.log_view.commits.is_empty() && !cache_matches_head {
            self.refresh_commit_log_after_head_change();
        } else if self.log_view.commits.is_empty() {
            // First entry with no cached pages: spawn a background
            // refresh fetch instead of loading on the UI thread. The
            // diff pane stays empty until the worker replies via
            // `apply_refresh_page`, which then loads the commit diff
            // for the freshly populated selection.
            self.cancel_commit_log_page_fetch();
            self.spawn_commit_log_refresh_fetch(None, None);
        } else {
            self.load_commit_diff_for_selected();
            self.maybe_prefetch_commit_log();
        }
    }

    /// Toggle the file-tree navigator: `<prefix> b` enters Tree mode from
    /// Status/Log and returns to Status from Tree. Mirrors `toggle_mode`'s
    /// fullscreen-reveal policy so the swap is visible behind a zoomed pane.
    pub fn toggle_tree_mode(&mut self) {
        let from = self.mode;
        let reveal_after_toggle = self.terminal.fullscreen || self.diff.fullscreen;
        if self.mode == ViewMode::Tree {
            self.exit_tree_to_status();
        } else {
            self.enter_tree_mode();
        }
        if reveal_after_toggle {
            self.focus_list();
        }
        tracing::debug!(from = ?from, to = ?self.mode, "tree mode toggled");
    }

    pub fn set_accent_index(&mut self, idx: usize) {
        // Normalize on entry so we never persist out-of-range indices to the
        // session file, even though `current_accent` would tolerate them.
        self.accent_idx = idx % crate::config::Accent::ALL.len();
    }

    pub fn cycle_accent(&mut self) {
        self.accent_idx = (self.accent_idx + 1) % crate::config::Accent::ALL.len();
    }

    pub fn current_accent(&self) -> ratatui::style::Color {
        crate::config::Accent::from_index(self.accent_idx).color()
    }

    pub fn cycle_focus_forward(&mut self) {
        if self.diff.fullscreen || self.list_fullscreen {
            return;
        }
        if self.terminal.fullscreen {
            let len = self.terminal.panes.len();
            if len > 0 {
                self.terminal.active = (self.terminal.active + 1) % len;
            }
            return;
        }
        match self.focus {
            Focus::FileList => {
                self.focus = Focus::DiffViewer;
            }
            Focus::DiffViewer => {
                if !self.terminal.panes.is_empty() {
                    self.terminal.active = 0;
                    self.focus = Focus::Terminal;
                } else {
                    self.focus = Focus::FileList;
                }
            }
            Focus::Terminal => {
                if self.terminal.active + 1 < self.terminal.panes.len() {
                    self.terminal.active += 1;
                } else {
                    self.focus = Focus::FileList;
                }
            }
        }
    }

    pub fn cycle_focus_backward(&mut self) {
        if self.diff.fullscreen || self.list_fullscreen {
            return;
        }
        if self.terminal.fullscreen {
            let len = self.terminal.panes.len();
            if len > 0 {
                self.terminal.active = (self.terminal.active + len - 1) % len;
            }
            return;
        }
        match self.focus {
            Focus::FileList => {
                if !self.terminal.panes.is_empty() {
                    self.terminal.active = self.terminal.panes.len() - 1;
                    self.focus = Focus::Terminal;
                } else {
                    self.focus = Focus::DiffViewer;
                }
            }
            Focus::DiffViewer => {
                self.focus = Focus::FileList;
            }
            Focus::Terminal => {
                if self.terminal.active > 0 {
                    self.terminal.active -= 1;
                } else {
                    self.focus = Focus::DiffViewer;
                }
            }
        }
    }

    pub fn toggle_terminal_fullscreen(&mut self) {
        if !self.terminal.fullscreen && self.terminal.panes.is_empty() {
            return;
        }
        self.terminal.fullscreen = !self.terminal.fullscreen;
        if self.terminal.fullscreen {
            self.focus = Focus::Terminal;
            self.diff.fullscreen = false;
            self.list_fullscreen = false;
        }
    }

    pub fn toggle_diff_fullscreen(&mut self) {
        self.diff.fullscreen = !self.diff.fullscreen;
        if self.diff.fullscreen {
            self.focus = Focus::DiffViewer;
            self.terminal.fullscreen = false;
            self.list_fullscreen = false;
        }
    }

    pub fn toggle_list_fullscreen(&mut self) {
        self.list_fullscreen = !self.list_fullscreen;
        if self.list_fullscreen {
            self.focus = Focus::FileList;
            self.diff.fullscreen = false;
            self.terminal.fullscreen = false;
        }
    }

    /// Jump focus to the file/commit list. Clears any fullscreen flag that
    /// would otherwise hide this pane; `list_fullscreen` itself stays so a
    /// user with the list already maximized keeps that view on F1.
    pub fn focus_list(&mut self) {
        self.focus = Focus::FileList;
        self.diff.fullscreen = false;
        self.terminal.fullscreen = false;
    }

    /// Jump focus to the diff viewer. Mirror policy of `focus_list`: clears
    /// the two competing fullscreens (`list_fullscreen`, `terminal.fullscreen`)
    /// and leaves `diff.fullscreen` alone so F2 preserves a zoomed diff.
    pub fn focus_diff(&mut self) {
        self.focus = Focus::DiffViewer;
        self.list_fullscreen = false;
        self.terminal.fullscreen = false;
    }
}
