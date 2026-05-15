use super::{App, Focus, ViewMode};
use crate::git::diff::load_commit_log;

impl App {
    pub fn toggle_mode(&mut self) {
        self.clear_diff_state();
        let from = self.mode;
        match self.mode {
            ViewMode::Status => {
                self.mode = ViewMode::Log;
                self.log_view.reset_drill_down();
                self.log_view.commit_scroll_x = 0;
                // Reuse cached pages on re-entry only while they still match
                // the latest HEAD observed by the snapshot worker. Status mode
                // intentionally does not refresh the hidden commit list, so a
                // HEAD change there must invalidate the cache on the next entry.
                let cached_head = self.log_view.commits.first().map(|c| c.oid);
                let cache_matches_head =
                    !self.log_view.commits.is_empty() && cached_head == self.last_head_oid;
                if !self.log_view.commits.is_empty() && !cache_matches_head {
                    self.refresh_commit_log_after_head_change();
                } else {
                    if self.log_view.commits.is_empty() {
                        self.cancel_commit_log_page_fetch();
                        let page_size = self.cfg_commit_log_page_size;
                        match self.with_repo(|repo| load_commit_log(repo, page_size)) {
                            Ok(commits) => {
                                self.log_view
                                    .set_commits_from_first_page(commits, page_size);
                                self.log_view.selected = 0;
                                // Sync last_head_oid to the freshly loaded HEAD so
                                // the next snapshot tick doesn't immediately
                                // re-trigger refresh_commit_log_after_head_change.
                                self.last_head_oid = self.log_view.commits.first().map(|c| c.oid);
                            }
                            Err(e) => {
                                tracing::warn!(error = %e, "failed to load commit log");
                                self.log_view.set_commits(Vec::new());
                                self.log_view.selected = 0;
                                self.status = Some(format!("git error: {e}"));
                            }
                        }
                    }
                    self.load_commit_diff_for_selected();
                    self.maybe_prefetch_commit_log();
                }
            }
            ViewMode::Log => {
                self.mode = ViewMode::Status;
                self.log_view.reset_drill_down();
                self.refresh_diff(true);
            }
        }
        tracing::debug!(from = ?from, to = ?self.mode, "view mode toggled");
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
