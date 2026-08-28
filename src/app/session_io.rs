use super::{App, Focus, NoticeKind, ViewMode};
use crate::git::diff::{load_commit_files, load_commit_log};
use crate::runtime::terminal::TerminalFullscreen;
use crate::workspace::persistence::SessionState;

impl App {
    // The live view, except for a selection still waiting on its first
    // snapshot — quitting before that arrives would otherwise record "no file
    // selected" over the one the user actually left open.
    pub fn session_to_save(&self) -> SessionState {
        let mut state = self.save_session();
        if let Some((path, scroll)) = self.git.view.pending_selection() {
            state.selected_file = Some(path.clone());
            state.scroll = *scroll;
        }
        state
    }

    pub fn save_session(&self) -> SessionState {
        SessionState {
            focus: Some(self.focus),
            selected_file: self
                .git
                .view
                .status
                .files
                .get(self.git.view.status.selected)
                .map(|f| f.path.clone()),
            scroll: self.git.view.diff.scroll,
            active_pane: self.terminal.active,
            terminal_fullscreen: self.terminal.fullscreen.fills_body(),
            diff_fullscreen: self.git.view.diff.fullscreen,
            list_fullscreen: self.list_fullscreen,
            mode: Some(self.git.view.mode),
            log_selected: self.git.view.log.selected,
            log_drill_down: self.git.view.log.drill_down,
            log_file_selected: self.git.view.log.file_selected,
            tree_selected_path: self.git.view.tree.selected_path(),
            tree_expanded: self.git.view.tree.expanded.iter().cloned().collect(),
        }
    }

    // Runs before the first snapshot so a fresh launch's terminal focus never
    // briefly draws — or routes keystrokes — over a restored list/diff focus.
    // Idempotent: `restore_session` re-applies it once the snapshot arrives.
    pub(crate) fn restore_pane_focus(&mut self, state: &SessionState) {
        // Pane-pointing state (active pane, terminal fullscreen, terminal
        // focus) means nothing until the session reports its panes, so hold it
        // in `pending_terminal` rather than quietly downgrading it against an empty list.
        self.pending_terminal = self.terminal.panes.is_empty().then(|| state.clone());
        self.terminal.active = state
            .active_pane
            .min(self.terminal.panes.len().saturating_sub(1));
        // `visible_start` isn't persisted (MVP scope) — recompute from the
        // restored active pane so the split-view window contains it.
        self.terminal.sync_visible_window();
        if let Some(focus) = state.focus {
            if focus == Focus::Terminal && self.terminal.panes.is_empty() {
                self.focus = Focus::FileList;
            } else {
                self.focus = focus;
            }
        }
        // Zoom is transient; a restored fullscreen session collapses to `Grid`
        // rather than persisting the zoom.
        self.terminal.fullscreen = if state.terminal_fullscreen && !self.terminal.panes.is_empty() {
            TerminalFullscreen::Grid
        } else {
            TerminalFullscreen::Off
        };
        if self.terminal.fullscreen.fills_body() {
            self.focus = Focus::Terminal;
        }
        self.git.view.diff.fullscreen =
            state.diff_fullscreen && !self.terminal.fullscreen.fills_body();
        if self.git.view.diff.fullscreen {
            self.focus = Focus::DiffViewer;
        }
        self.list_fullscreen = state.list_fullscreen
            && !self.terminal.fullscreen.fills_body()
            && !self.git.view.diff.fullscreen;
        if self.list_fullscreen {
            self.focus = Focus::FileList;
        }
    }

    // Runs as soon as the session loads, not on the first snapshot: only
    // Status's selection needs snapshot data, so it waits in
    // `pending_selection` — an empty list can't collide with user input.
    pub fn restore_session(&mut self, state: &SessionState) {
        self.restore_pane_focus(state);

        // Avoid loading a workdir diff when the saved mode is Log — otherwise
        // we'd waste a load and clamp the scroll against the wrong diff length.
        match state.mode {
            Some(ViewMode::Log) => self.restore_log_session(state),
            Some(ViewMode::Tree) => self.restore_tree_session(state),
            _ if self.git.view.status.files.is_empty() => {
                self.git.view.set_pending_selection(
                    state.selected_file.clone().map(|path| (path, state.scroll)),
                );
            }
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
            && let Some(idx) = self
                .git
                .view
                .status
                .files
                .iter()
                .position(|f| &f.path == path)
        {
            self.git.view.status.selected = idx;
            self.refresh_diff(true);
            self.git.view.diff.scroll = state.scroll.min(self.git.view.diff.max_scroll());
        }
        // If the saved file is gone, leave selected/scroll as they were after
        // the initial snapshot — applying saved_scroll to a different file
        // would jump the user to an unrelated location.
    }

    fn restore_tree_session(&mut self, state: &SessionState) {
        self.git.view.set_mode(ViewMode::Tree);
        // A status search started before this restore (e.g. `/` pressed while
        // the default Status view awaited the first snapshot) would otherwise
        // stay active and capture Tree keystrokes. Drop it.
        self.git.view.status.cancel_search();
        self.clear_diff_state();
        // Restoring expansion mutates the cache/expanded set; drop the stale
        // row-width bound so horizontal scroll clamps to the restored rows.
        self.git.view.tree.row_width_cache.set(None);
        // The session file is an on-disk boundary: keep only safe
        // repo-internal relative paths, so a hand-edited `..` or absolute path
        // can't drive a directory read outside the working tree.
        // (`refresh_tree_cache` prunes entries missing on disk.)
        self.git.view.tree.expanded = state
            .tree_expanded
            .iter()
            .filter(|p| crate::ui::tree_view::is_safe_rel_path(p))
            .cloned()
            .collect();
        self.refresh_tree_cache();
        if let Some(path) = &state.tree_selected_path {
            let rows = self.git.view.tree.visible_rows();
            if let Some(idx) = rows.iter().position(|r| &r.path == path) {
                self.git.view.tree.selected = idx;
            }
        }
        let row_count = self.git.view.tree.visible_rows().len();
        self.git.view.tree.clamp_selection(row_count);
        self.preview_tree_selected();
    }

    fn restore_log_session(&mut self, state: &SessionState) {
        // A page worker launched before the restore would race against the
        // fresh `set_commits` below: its reply would be silently appended over
        // the restored list. Cancel before mutating state.
        self.cancel_commit_log_page_fetch();
        let page_size = self.git.commit_log.page_size();
        let commits = match self.with_repo(|repo| load_commit_log(repo, page_size)) {
            Ok(c) => c,
            Err(e) => {
                tracing::warn!(error = %e, "failed to restore commit log");
                return;
            }
        };
        let fully_loaded = commits.len() < page_size;
        self.git.view.log.set_commits(commits);
        self.git.view.log.fully_loaded = fully_loaded;
        self.git.view.log.selected = state
            .log_selected
            .min(self.git.view.log.commits.len().saturating_sub(1));
        // Avoid a same-tick HEAD-change-trigger reload on the next snapshot.
        self.git
            .commit_log
            .set_last_head_oid(self.git.view.log.commits.first().map(|c| c.oid));
        self.git.view.set_mode(ViewMode::Log);

        if state.log_drill_down {
            self.restore_log_drill_down(state);
        } else {
            self.load_commit_diff_for_selected();
        }
        self.git.load_controller.restore_diff_scroll(state.scroll);
        // Restored cursor may already sit close to the tail of the first page;
        // kick off the next prefetch so the first key move doesn't bump into a
        // not-yet-loaded boundary.
        self.maybe_prefetch_commit_log();
    }

    fn restore_log_drill_down(&mut self, state: &SessionState) {
        let (oid, title) = match self.git.view.log.commits.get(self.git.view.log.selected) {
            Some(entry) => (entry.oid, entry.to_string()),
            None => {
                // Saved drill-down pointed at a commit no longer in the loaded
                // first page (history rewrite, force-push) — surface why the
                // user is back at the commit-level view, not where they left off.
                tracing::warn!(
                    selected = self.git.view.log.selected,
                    "drill-down restore: saved commit index is out of range"
                );
                self.raise_notice(
                    NoticeKind::Session,
                    "drill-down restore skipped: saved commit not in log",
                );
                self.load_commit_diff_for_selected();
                return;
            }
        };
        match self.with_repo(|repo| load_commit_files(repo, oid)) {
            Ok(files) => {
                self.git.view.log.set_commit_files(files);
                self.git.view.log.drill_down = true;
                if self.git.view.log.commit_files.is_empty() {
                    self.git.view.log.file_selected = 0;
                    self.clear_diff_state();
                    self.git.view.log.diff_title = title;
                } else {
                    self.git.view.log.file_selected = state
                        .log_file_selected
                        .min(self.git.view.log.commit_files.len().saturating_sub(1));
                    self.load_file_diff_for_log_file_selected();
                }
            }
            Err(e) => {
                tracing::warn!(error = %e, "failed to load drill-down commit files");
                self.raise_notice(
                    NoticeKind::Session,
                    format!("drill-down restore failed: {e}"),
                );
                self.load_commit_diff_for_selected();
            }
        }
    }
}
