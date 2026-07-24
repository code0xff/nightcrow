use super::{App, DiffPaneView, FileViewState, NoticeKind, ViewMode};
use crate::git::diff::{DiffHunk, load_file_diff, parse_hunk_new_start};

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
    /// caller can surface them as a notice.
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
        // Only the working-tree status view shows a file diff. Log drives the
        // diff via its own loaders, and Tree shows raw file previews — neither
        // should have a status diff loaded over it.
        if self.mode != ViewMode::Status {
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
            self.raise_notice(NoticeKind::Diff, e.to_string());
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
                // Clear any stale diff error from a previous failed load —
                // keeping it would mislead the user about the current file's
                // state. Notices of other kinds are left alone.
                self.clear_notice(NoticeKind::Diff);
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
                // already surfaced as a notice by the loader.
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