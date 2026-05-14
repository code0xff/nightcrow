use super::{App, DiffPaneView, FileViewKey, FileViewState, ViewMode};
use crate::git::diff::{
    DiffHunk, load_commit_diff, load_commit_file_blob, load_commit_file_diff, load_commit_log_page,
    load_file_diff, load_workdir_file, parse_hunk_new_start,
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
        f(self.repo_cache.as_ref().unwrap())
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
                    }
                }
                if !self.diff.search.query.is_empty() {
                    self.diff.recompute_matches(reset_scroll);
                }
            }
            Err(_) => {
                self.clear_diff_state();
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
        self.diff.cached_syntax_name = None;
        self.diff.search.matches.clear();
        self.diff.search.cursor = 0;
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
                let path = self
                    .log_view
                    .commit_files
                    .get(self.log_view.file_selected)?
                    .path
                    .clone();
                Some(FileViewKey::Commit { oid, path })
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
            FileViewKey::Commit { oid, path } => {
                let oid = *oid;
                self.with_repo(|repo| load_commit_file_blob(repo, oid, path))
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
                fv.scroll = anchor
                    .map(|n| n.saturating_sub(1).saturating_sub(2))
                    .unwrap_or(0);
                fv.total_lines = if content.is_empty() {
                    0
                } else {
                    content.lines().count()
                };
                fv.content = content;
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
        }
        self.apply_diff_result(result, DiffApply::ResetWithTitle(&title));
    }

    /// Reload the Log view's commit list after the snapshot worker detected a
    /// HEAD oid change (new commit via the terminal pane, external push,
    /// amend, branch switch). Preserves the selection by oid so the user
    /// keeps looking at the same commit when one is appended above; falls
    /// back to the freshest commit when the prior oid disappeared (amend/
    /// force-push). Drill-down stays if its commit survives, otherwise it
    /// collapses to the commit-level diff view.
    pub(crate) fn refresh_commit_log_after_head_change(&mut self) {
        let prior_selected_oid = self
            .log_view
            .commits
            .get(self.log_view.selected)
            .map(|c| c.oid);
        let prior_head_oid = self.log_view.commits.first().map(|c| c.oid);

        // Any in-flight worker was launched against the old head; its result
        // would be `skip`-mismatched after we prepend or reset below. Drop it.
        self.cancel_commit_log_page_fetch();

        let page_size = self.cfg_commit_log_page_size;
        let page = match self.with_repo(|repo| load_commit_log_page(repo, 0, page_size)) {
            Ok(c) => c,
            Err(e) => {
                tracing::warn!(error = %e, "failed to refresh commit log after HEAD change");
                return;
            }
        };

        // If the previous head still appears in the freshly fetched first
        // page, treat the change as a fast-forward / new commit: prepend the
        // newer entries onto the existing list so all accumulated pages stay
        // valid. Otherwise (rewrite, branch switch, force push, no prior list)
        // discard everything and start from the new first page.
        let prepend_idx = prior_head_oid.and_then(|oid| page.iter().position(|c| c.oid == oid));
        if let Some(idx) = prepend_idx
            && !self.log_view.commits.is_empty()
        {
            let mut new_head_commits: Vec<_> = page.into_iter().take(idx).collect();
            let n_new = new_head_commits.len();
            new_head_commits.extend(self.log_view.commits.drain(..));
            self.log_view.commits = new_head_commits;
            self.log_view.loaded_count = self.log_view.commits.len();
            self.log_view.commit_width_cache.set(None);
            // Slide the selection so the user keeps looking at the same
            // commit even though new entries appeared above it.
            if let Some(prior_oid) = prior_selected_oid
                && let Some(pos) = self
                    .log_view
                    .commits
                    .iter()
                    .position(|c| c.oid == prior_oid)
            {
                self.log_view.selected = pos;
            } else {
                self.log_view.selected = self.log_view.selected.saturating_add(n_new);
            }
        } else {
            let fully_loaded = page.len() < page_size;
            self.log_view.set_commits(page);
            self.log_view.fully_loaded = fully_loaded;
            self.log_view.selected = prior_selected_oid
                .and_then(|oid| self.log_view.commits.iter().position(|c| c.oid == oid))
                .unwrap_or(0);
        }
        self.log_view.commit_scroll_x = 0;

        // Drill-down survives only if the commit it was opened on is still in
        // the (possibly extended) list. Otherwise drop back to the commit-
        // level diff.
        if self.log_view.drill_down
            && prior_selected_oid
                .is_none_or(|oid| !self.log_view.commits.iter().any(|c| c.oid == oid))
        {
            self.log_view.reset_drill_down();
        }

        if self.log_view.drill_down {
            self.load_file_diff_for_log_file_selected();
        } else {
            self.load_commit_diff_for_selected();
        }

        self.maybe_prefetch_commit_log();
    }
}
