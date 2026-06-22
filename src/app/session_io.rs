use super::{App, Focus, ViewMode};
use crate::git::diff::{load_commit_files, load_commit_log};
use crate::session::SessionState;

impl App {
    pub fn set_pending_session(&mut self, state: SessionState) {
        self.pending_session = Some(state);
    }

    pub fn save_session(&self) -> SessionState {
        SessionState {
            focus: Some(self.focus),
            selected_file: self
                .status_view
                .files
                .get(self.status_view.selected)
                .map(|f| f.path.clone()),
            scroll: self.diff.scroll,
            active_pane: self.terminal.active,
            terminal_fullscreen: self.terminal.fullscreen,
            diff_fullscreen: self.diff.fullscreen,
            list_fullscreen: self.list_fullscreen,
            mode: Some(self.mode),
            log_selected: self.log_view.selected,
            accent_idx: self.accent_idx,
            log_drill_down: self.log_view.drill_down,
            log_file_selected: self.log_view.file_selected,
            tree_selected_path: self.tree_view.selected_path(),
            tree_expanded: self.tree_view.expanded.iter().cloned().collect(),
        }
    }

    /// Restore the pane/focus/fullscreen subset of a saved session. This needs
    /// no loaded snapshot data, so it runs synchronously at startup (before the
    /// first snapshot) to stop the fresh-launch terminal focus from briefly
    /// drawing — and routing keystrokes — over a saved `FileList`/`DiffViewer`
    /// focus on restart. Idempotent: `restore_session` re-applies it once the
    /// snapshot arrives, which is a no-op against the same state.
    pub(crate) fn restore_pane_focus(&mut self, state: &SessionState) {
        self.terminal.active = state
            .active_pane
            .min(self.terminal.panes.len().saturating_sub(1));
        if let Some(focus) = state.focus {
            if focus == Focus::Terminal && self.terminal.panes.is_empty() {
                self.focus = Focus::FileList;
            } else {
                self.focus = focus;
            }
        }
        self.terminal.fullscreen = state.terminal_fullscreen && !self.terminal.panes.is_empty();
        if self.terminal.fullscreen {
            self.focus = Focus::Terminal;
        }
        self.diff.fullscreen = state.diff_fullscreen && !self.terminal.fullscreen;
        if self.diff.fullscreen {
            self.focus = Focus::DiffViewer;
        }
        self.list_fullscreen =
            state.list_fullscreen && !self.terminal.fullscreen && !self.diff.fullscreen;
        if self.list_fullscreen {
            self.focus = Focus::FileList;
        }
    }

    pub fn restore_session(&mut self, state: &SessionState) {
        // Pane / focus / fullscreen restoration — independent of view mode.
        self.restore_pane_focus(state);
        self.set_accent_index(state.accent_idx);

        // Mode-specific diff/scroll restoration. We avoid loading a workdir diff
        // when the saved mode is Log — otherwise we'd waste a load and clamp the
        // scroll against the wrong diff length.
        match state.mode {
            Some(ViewMode::Log) => self.restore_log_session(state),
            Some(ViewMode::Tree) => self.restore_tree_session(state),
            _ => self.restore_status_session(state),
        }

        tracing::debug!(
            focus = ?state.focus,
            file = ?state.selected_file,
            scroll = state.scroll,
            mode = ?state.mode,
            drill = state.log_drill_down,
            "session restored"
        );
    }

    fn restore_status_session(&mut self, state: &SessionState) {
        if let Some(path) = &state.selected_file
            && let Some(idx) = self.status_view.files.iter().position(|f| &f.path == path)
        {
            self.status_view.selected = idx;
            self.refresh_diff(true);
            self.diff.scroll = state.scroll.min(self.diff.max_scroll());
        }
        // If the saved file is no longer present, leave selected/scroll as they
        // were after the initial snapshot — applying saved_scroll to a different
        // file would jump the user to an unrelated location.
    }

    fn restore_tree_session(&mut self, state: &SessionState) {
        self.mode = ViewMode::Tree;
        // A status search started before this restore (e.g. `/` pressed while
        // the default Status view was shown awaiting the first snapshot) would
        // otherwise stay active and capture Tree keystrokes. Drop it; the diff
        // search overlay is cleared by `clear_diff_state` below.
        self.status_view.cancel_search();
        self.clear_diff_state();
        // Restoring expansion mutates the cache/expanded set; drop the stale
        // row-width bound so horizontal scroll clamps to the restored rows.
        self.tree_view.row_width_cache.set(None);
        self.ensure_tree_root();
        // Re-expand saved directories shallowest-first so each parent is loaded
        // before its children are referenced by `visible_rows`. The session
        // file is an on-disk boundary: drop any entry that isn't a safe
        // repo-internal relative path so a hand-edited/corrupted `..` or
        // absolute path can't drive a directory read outside the working tree.
        let mut dirs: Vec<String> = state
            .tree_expanded
            .iter()
            .filter(|p| crate::ui::tree_view::is_safe_rel_path(p))
            .cloned()
            .collect();
        dirs.sort_by_key(|p| p.matches('/').count());
        for dir in dirs {
            self.ensure_tree_children(&dir);
            self.tree_view.expanded.insert(dir);
        }
        // Restore the cursor by path when it still resolves to a visible row;
        // otherwise leave it at the top.
        if let Some(path) = &state.tree_selected_path {
            let rows = self.tree_view.visible_rows();
            if let Some(idx) = rows.iter().position(|r| &r.path == path) {
                self.tree_view.selected = idx;
            }
        }
        let row_count = self.tree_view.visible_rows().len();
        self.tree_view.clamp_selection(row_count);
        self.preview_tree_selected();
    }

    fn restore_log_session(&mut self, state: &SessionState) {
        // A page worker launched before the restore (e.g. via `toggle_mode`
        // earlier in this frame) would race against the fresh `set_commits`
        // below: its reply would be matched by `loaded_count` and silently
        // appended over the restored list. Cancel before we mutate state.
        self.cancel_commit_log_page_fetch();
        let page_size = self.pagination.page_size;
        let commits = match self.with_repo(|repo| load_commit_log(repo, page_size)) {
            Ok(c) => c,
            Err(e) => {
                tracing::warn!(error = %e, "failed to restore commit log");
                return;
            }
        };
        let fully_loaded = commits.len() < page_size;
        self.log_view.set_commits(commits);
        self.log_view.fully_loaded = fully_loaded;
        self.log_view.selected = state
            .log_selected
            .min(self.log_view.commits.len().saturating_sub(1));
        // Same rationale as toggle_mode: avoid a same-tick HEAD-change-trigger
        // reload on the very next snapshot.
        self.pagination.last_head_oid = self.log_view.commits.first().map(|c| c.oid);
        self.mode = ViewMode::Log;

        if state.log_drill_down {
            self.restore_log_drill_down(state);
        } else {
            self.load_commit_diff_for_selected();
        }
        self.diff.scroll = state.scroll.min(self.diff.max_scroll());
        // Restored cursor may already sit close to the tail of the first
        // page; kick off the next prefetch so the first key move doesn't
        // bump into a not-yet-loaded boundary.
        self.maybe_prefetch_commit_log();
    }

    fn restore_log_drill_down(&mut self, state: &SessionState) {
        let (oid, title) = match self.log_view.commits.get(self.log_view.selected) {
            Some(entry) => (entry.oid, entry.to_string()),
            None => {
                // Saved drill-down pointed at a commit that's no longer in
                // the loaded first page (history rewrite, force-push) —
                // surface this so the user knows why they're back at the
                // commit-level view instead of where they left off.
                tracing::warn!(
                    selected = self.log_view.selected,
                    "drill-down restore: saved commit index is out of range"
                );
                self.status =
                    Some("drill-down restore skipped: saved commit not in log".to_string());
                self.load_commit_diff_for_selected();
                return;
            }
        };
        match self.with_repo(|repo| load_commit_files(repo, oid)) {
            Ok(files) => {
                self.log_view.set_commit_files(files);
                self.log_view.drill_down = true;
                if self.log_view.commit_files.is_empty() {
                    self.log_view.file_selected = 0;
                    self.clear_diff_state();
                    self.log_view.diff_title = title;
                } else {
                    self.log_view.file_selected = state
                        .log_file_selected
                        .min(self.log_view.commit_files.len().saturating_sub(1));
                    self.load_file_diff_for_log_file_selected();
                }
            }
            Err(e) => {
                tracing::warn!(error = %e, "failed to load drill-down commit files");
                self.status = Some(format!("drill-down restore failed: {e}"));
                self.load_commit_diff_for_selected();
            }
        }
    }
}
