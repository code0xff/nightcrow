use super::{App, DiffPaneView, FileViewState, NoticeKind, ViewMode};
use crate::git::diff::{DiffHunk, load_file_diff, parse_hunk_new_start};

// Replaces a prior 3-flag signature where `reset_scroll`/`keep_scroll` were
// hard to parse at call sites.
pub(crate) enum DiffApply<'a> {
    Reset,
    KeepScroll(usize),
    ResetWithTitle(&'a str),
}

impl App {
    pub fn reload_diff(&mut self) {
        self.refresh_diff(true);
    }

    // Opens the cached `git2::Repository` lazily; invalidated by `change_repo`.
    // Only drops the handle on errors suggesting the repo itself is gone (the
    // motivating case: `rm -rf .git && git init` in the terminal pane) — normal
    // data misses like "object not found" must not force a fresh `discover` walk.
    pub(crate) fn with_repo<R>(
        &mut self,
        f: impl FnOnce(&git2::Repository) -> anyhow::Result<R>,
    ) -> anyhow::Result<R> {
        if self.repo_cache.is_none() {
            let repo = git2::Repository::discover(self.repo_path.as_str())
                .map_err(|e| anyhow::anyhow!("{}", crate::git::format_discover_error(&e)))?;
            self.repo_cache = Some(repo);
        }
        let result = f(self.repo_cache.as_ref().unwrap());
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
        // Only Status shows a file diff; Log and Tree drive the diff via their
        // own loaders and must not have a status diff loaded over them.
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

    pub(crate) fn apply_diff_result(
        &mut self,
        result: anyhow::Result<Vec<DiffHunk>>,
        mode: DiffApply<'_>,
    ) {
        let reset_scroll = matches!(mode, DiffApply::Reset | DiffApply::ResetWithTitle(_));
        match result {
            Ok(hunks) => {
                self.clear_notice(NoticeKind::Diff);
                self.diff.set_hunks(hunks);
                match mode {
                    DiffApply::Reset | DiffApply::ResetWithTitle(_) => {
                        self.diff.scroll = 0;
                        self.diff.scroll_x = 0;
                        self.diff.search.cursor = 0;
                        self.invalidate_file_view();
                    }
                    DiffApply::KeepScroll(prev) => {
                        // Clamp against the new (possibly shorter) diff so
                        // scroll isn't left out of range for the next keystroke.
                        self.diff.scroll = prev.min(self.diff.max_scroll());
                        // The file-overlay anchor was computed against the
                        // previous hunks; recompute so the open file pane stays
                        // aligned with the replaced diff.
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
                // KeepScroll error (in-place refresh) keeps the prior diff:
                // usually a transient race (mid-rename, slow index update) and
                // clearing would flash an empty pane and dangle `scroll`.
                if !matches!(mode, DiffApply::KeepScroll(_)) {
                    self.clear_diff_state();
                }
            }
        }
        // Title belongs to the surrounding view, not the diff state — set it
        // last so it survives both success and failure.
        if let DiffApply::ResetWithTitle(title) = mode {
            self.log_view.diff_title = title.to_string();
        }
    }

    pub(crate) fn clear_diff_state(&mut self) {
        self.diff.set_hunks(Vec::new());
        // Drop the entire search state, not just the match list: keeping the
        // query alive after a content-discarding clear would leave a ghost
        // `[0/0]` counter and apply the previous file's query to unrelated
        // content on the next load.
        self.diff.search.clear();
        self.diff.scroll = 0;
        self.diff.scroll_x = 0;
        self.invalidate_file_view();
    }

    pub(crate) fn invalidate_file_view(&mut self) {
        self.diff.view = DiffPaneView::Diff;
        self.diff.file_view = FileViewState::default();
    }

    pub(crate) fn anchor_for_current_diff(&self) -> Option<usize> {
        let scroll = self.diff.scroll;
        let mut offset = 0usize;
        let mut chosen = None;
        for h in self.diff.hunks() {
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

    // Spawns a background fetch; the merge happens in `apply_refresh_page` when
    // the worker replies so the UI tick never blocks on a 100-commit revwalk.
    pub(crate) fn refresh_commit_log_after_head_change(&mut self) {
        let prior_selected_oid = self
            .log_view
            .commits
            .get(self.log_view.selected)
            .map(|c| c.oid);
        let prior_head_oid = self.log_view.commits.first().map(|c| c.oid);

        // Any in-flight worker was launched against state that no longer
        // matches; drop it so only this refresh's reply can land.
        self.cancel_commit_log_page_fetch();
        self.spawn_commit_log_refresh_fetch(prior_selected_oid, prior_head_oid);
    }
}
