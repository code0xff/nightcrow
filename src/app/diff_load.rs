use super::{App, DiffPaneView, FileViewKey, FileViewState, ViewMode};
use crate::git::diff::{
    DiffHunk, load_commit_diff, load_commit_file_blob, load_commit_file_diff, load_file_diff,
    load_workdir_file, parse_hunk_new_start,
};

/// Post-load behaviour for `apply_diff_result`. Replaces the prior 3-flag
/// signature where the combination of `reset_scroll` and `keep_scroll` was
/// hard to parse at call sites.
pub(crate) enum DiffApply<'a> {
    /// Reset scroll/cursor to top after a successful load.
    Reset,
    /// Keep the previous scroll position (for in-place refresh).
    KeepScroll(usize),
    /// Reset scroll and additionally update the log diff title.
    ResetWithTitle(&'a str),
}

impl App {
    pub fn reload_diff(&mut self) {
        self.refresh_diff(true);
    }

    /// Run `f` with the cached `git2::Repository`, opening it lazily on first
    /// use. Cache is invalidated by `change_repo` so that follow-up calls open
    /// a fresh handle for the new path. Errors from the open propagate so the
    /// caller can surface them in `self.status`.
    pub(crate) fn with_repo<R>(
        &mut self,
        f: impl FnOnce(&git2::Repository) -> anyhow::Result<R>,
    ) -> anyhow::Result<R> {
        if self.repo_cache.is_none() {
            let repo = git2::Repository::discover(self.repo_path.as_str())
                .map_err(|e| anyhow::anyhow!("not a git repository: {e}"))?;
            self.repo_cache = Some(repo);
        }
        // unwrap is sound: we just inserted Some above when None.
        let result = f(self.repo_cache.as_ref().unwrap());
        // Only drop the cached handle when the error suggests the repo
        // *itself* is gone or unreadable — a user doing `rm -rf .git && git
        // init` in the terminal pane is the motivating case. Errors like
        // "path not in commit" or "object not found" are normal data misses
        // that shouldn't force a fresh `Repository::discover` walk on every
        // subsequent call.
        if let Err(ref e) = result
            && let Some(git_err) = e.downcast_ref::<git2::Error>()
            && matches!(
                git_err.class(),
                git2::ErrorClass::Os | git2::ErrorClass::Repository
            )
        {
            self.repo_cache = None;
        }
        result
    }

    pub(crate) fn refresh_diff(&mut self, reset_scroll: bool) {
        if self.mode == ViewMode::Log {
            return;
        }
        let previous_scroll = self.diff.scroll;
        let Some(path) = self.selected_filtered_status_path() else {
            self.clear_diff_state();
            return;
        };
        let result = self.with_repo(|repo| load_file_diff(repo, &path));
        if let Err(e) = &result {
            tracing::warn!(error = %e, file = %path, "failed to load diff");
            self.status = Some(format!("diff error: {e}"));
        }
        let mode = if reset_scroll {
            DiffApply::Reset
        } else {
            DiffApply::KeepScroll(previous_scroll)
        };
        self.apply_diff_result(result, mode);
    }

    /// Centralizes the post-load shape used by every diff loader: on success
    /// stash hunks, reset/restore scroll and search cursor, optionally update
    /// the log title, and recompute diff search matches; on error clear state
    /// but preserve the title so the user knows what failed.
    pub(crate) fn apply_diff_result(
        &mut self,
        result: anyhow::Result<Vec<DiffHunk>>,
        mode: DiffApply<'_>,
    ) {
        let reset_scroll = matches!(mode, DiffApply::Reset | DiffApply::ResetWithTitle(_));
        match result {
            Ok(hunks) => {
                // Clear any stale "diff error:" surfaced by a previous failed
                // load — keeping it would mislead the user about the current
                // file's state. Untouched for unrelated status messages.
                if self
                    .status
                    .as_deref()
                    .is_some_and(|m| m.starts_with("diff error:"))
                {
                    self.status = None;
                }
                self.diff.hunks = hunks;
                self.diff.rebuild_lower_cache();
                match mode {
                    DiffApply::Reset | DiffApply::ResetWithTitle(_) => {
                        self.diff.scroll = 0;
                        self.diff.scroll_x = 0;
                        self.diff.search.cursor = 0;
                        self.invalidate_file_view();
                    }
                    DiffApply::KeepScroll(prev) => {
                        // New hunks may be shorter than the prior load, so
                        // clamp against the freshly assigned diff to avoid
                        // leaving an out-of-range scroll that misbehaves on
                        // the next navigation keystroke.
                        self.diff.scroll = prev.min(self.diff.max_scroll());
                        // If the file-overlay view is open, the anchor was
                        // computed against the previous hunks. After the
                        // diff is replaced, that anchor may point at the
                        // wrong row — recompute against the new hunks so
                        // the open file pane stays aligned with the diff.
                        if self.diff.file_view.key.is_some() {
                            self.diff.file_view.anchor_line = self.anchor_for_current_diff();
                        }
                    }
                }
                if !self.diff.search.query.is_empty() {
                    self.diff.recompute_matches(reset_scroll);
                }
            }
            Err(_) => {
                // For a KeepScroll error (an in-place refresh of the same
                // file) we keep the prior diff on screen: this is usually a
                // transient race (mid-rename, slow git index update) and
                // clearing would both flash an empty pane and leave `scroll`
                // dangling past the now-empty `max_scroll`. The error is
                // already surfaced in `self.status` by the loader.
                if !matches!(mode, DiffApply::KeepScroll(_)) {
                    self.clear_diff_state();
                }
            }
        }
        // Title belongs to the surrounding view, not the diff state — set it
        // last so it survives both success and failure of the load.
        if let DiffApply::ResetWithTitle(title) = mode {
            self.log_view.diff_title = title.to_string();
        }
    }

    pub(crate) fn clear_diff_state(&mut self) {
        self.diff.hunks.clear();
        self.diff.hunks_lines_lower.clear();
        self.diff.line_highlights.clear();
        self.diff.cached_hunk_syntax.clear();
        // Drop the entire search state, not just the match list: keeping the
        // query alive after a content-discarding clear would (a) leave a
        // ghost `[0/0]` counter visible in the title, and (b) cause the
        // next file load's `recompute_matches` to apply the previous file's
        // query to unrelated content. `search.clear` also flips `active`
        // off so the search bar disappears in the same frame.
        self.diff.search.clear();
        self.diff.scroll = 0;
        self.diff.scroll_x = 0;
        self.invalidate_file_view();
    }

    pub(crate) fn invalidate_file_view(&mut self) {
        self.diff.view = DiffPaneView::Diff;
        self.diff.file_view = FileViewState::default();
    }

    pub(crate) fn current_file_view_key(&self) -> Option<FileViewKey> {
        match self.mode {
            ViewMode::Status => {
                let path = self.selected_filtered_status_file()?.path.clone();
                Some(FileViewKey::Status(path))
            }
            ViewMode::Log => {
                if !self.log_view.drill_down {
                    return None;
                }
                let oid = self.log_view.commits.get(self.log_view.selected)?.oid;
                let file = self
                    .log_view
                    .commit_files
                    .get(self.log_view.file_selected)?;
                Some(FileViewKey::Commit {
                    oid,
                    path: file.path.clone(),
                    // Commit deltas carry their single status in the index
                    // column; `load_commit_file_blob` only needs the Deleted
                    // case to read from the parent tree.
                    status: file.index,
                })
            }
        }
    }

    /// Pick the new-side starting line of the hunk currently visible at the
    /// top of the diff viewport. Walks the flat hunk layout (one header row +
    /// body rows per hunk) and returns the most recent hunk whose header was
    /// reached at or before `self.diff.scroll`. Falls back to the first
    /// parseable hunk when the scroll is past every hunk we could parse.
    pub(crate) fn anchor_for_current_diff(&self) -> Option<usize> {
        let scroll = self.diff.scroll;
        let mut offset = 0usize;
        let mut chosen = None;
        for h in &self.diff.hunks {
            if let Some(n) = parse_hunk_new_start(&h.header) {
                chosen = Some(n);
            }
            offset += 1 + h.lines.len();
            if scroll < offset {
                break;
            }
        }
        chosen
    }

    pub(crate) fn load_file_view(&mut self, key: FileViewKey) {
        let result = match &key {
            FileViewKey::Status(path) => self.with_repo(|repo| load_workdir_file(repo, path)),
            FileViewKey::Commit {
                oid, path, status, ..
            } => {
                let oid = *oid;
                let status = *status;
                self.with_repo(|repo| load_commit_file_blob(repo, oid, path, status))
            }
        };
        let anchor = self.anchor_for_current_diff();
        let mut fv = FileViewState {
            key: Some(key),
            anchor_line: anchor,
            ..Default::default()
        };
        match result {
            Ok(content) => {
                fv.set_content(content);
                // Initial scroll: 2 lines of context above the hunk's new-side
                // start line, converted from 1-based to 0-based. Clamp against
                // `max_scroll` so a stale anchor past the current file length
                // (file truncated since the diff was computed) doesn't open
                // the file view on a blank region the user has to page back from.
                let initial = anchor
                    .map(|n| n.saturating_sub(1).saturating_sub(2))
                    .unwrap_or(0);
                fv.scroll = initial.min(fv.max_scroll());
            }
            Err(e) => {
                fv.error = Some(e.to_string());
            }
        }
        self.diff.file_view = fv;
    }

    pub fn toggle_diff_file_view(&mut self) {
        if self.diff.view == DiffPaneView::File {
            self.diff.view = DiffPaneView::Diff;
            return;
        }
        let Some(key) = self.current_file_view_key() else {
            return;
        };
        if self.diff.file_view.key.as_ref() != Some(&key) {
            self.load_file_view(key);
        }
        self.diff.view = DiffPaneView::File;
    }

    pub(crate) fn load_commit_diff_for_selected(&mut self) {
        let (oid, title) = match self.log_view.commits.get(self.log_view.selected) {
            Some(entry) => (entry.oid, entry.to_string()),
            None => {
                self.clear_diff_state();
                self.log_view.diff_title.clear();
                return;
            }
        };
        let result = self.with_repo(|repo| load_commit_diff(repo, oid));
        if let Err(e) = &result {
            tracing::warn!(error = %e, "failed to load commit diff");
            self.status = Some(format!("diff error: {e}"));
        }
        self.apply_diff_result(result, DiffApply::ResetWithTitle(&title));
    }

    pub(crate) fn load_file_diff_for_log_file_selected(&mut self) {
        let Some((oid, short_id, commit_title)) = self
            .log_view
            .commits
            .get(self.log_view.selected)
            .map(|c| (c.oid, c.short_id.clone(), c.to_string()))
        else {
            self.clear_diff_state();
            self.log_view.diff_title.clear();
            return;
        };
        let Some(path) = self
            .log_view
            .commit_files
            .get(self.log_view.file_selected)
            .map(|f| f.path.clone())
        else {
            self.clear_diff_state();
            self.log_view.diff_title = commit_title;
            return;
        };
        let title = format!("{short_id} {path}");
        let result = self.with_repo(|repo| load_commit_file_diff(repo, oid, &path));
        if let Err(e) = &result {
            tracing::warn!(error = %e, file = %path, "failed to load commit file diff");
            self.status = Some(format!("diff error: {e}"));
        }
        self.apply_diff_result(result, DiffApply::ResetWithTitle(&title));
    }

    /// Reload the Log view's commit list after the snapshot worker detected a
    /// HEAD oid change (new commit via the terminal pane, external push,
    /// amend, branch switch). Captures the current selection/head oids and
    /// spawns a background fetch; the merge happens in `apply_refresh_page`
    /// when the worker replies so the UI tick never blocks on a 100-commit
    /// revwalk. Selection-by-oid preservation, prepend-vs-reset detection,
    /// and drill-down survival all live on that arrival path.
    pub(crate) fn refresh_commit_log_after_head_change(&mut self) {
        let prior_selected_oid = self
            .log_view
            .commits
            .get(self.log_view.selected)
            .map(|c| c.oid);
        let prior_head_oid = self.log_view.commits.first().map(|c| c.oid);

        // Any in-flight worker (tail prefetch or older refresh) was launched
        // against state that no longer matches; drop it so only this fresh
        // refresh's reply can land.
        self.cancel_commit_log_page_fetch();
        self.spawn_commit_log_refresh_fetch(prior_selected_oid, prior_head_oid);
    }
}
