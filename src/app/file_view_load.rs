use super::diff_load::DiffApply;
use super::{App, DiffPaneView, FileViewKey, FileViewState, NoticeKind, ViewMode};
use crate::git::diff::{
    load_commit_diff, load_commit_file_blob, load_commit_file_diff, load_workdir_file,
};

impl App {
    pub(crate) fn current_file_view_key(&self) -> Option<FileViewKey> {
        match self.mode {
            ViewMode::Status => {
                let path = self.selected_filtered_status_file()?.path.clone();
                Some(FileViewKey::Status(path))
            }
            ViewMode::Tree => {
                let row = self
                    .tree_view
                    .visible_rows()
                    .into_iter()
                    .nth(self.tree_view.selected)?;
                if row.is_dir {
                    return None;
                }
                Some(FileViewKey::Status(row.path))
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
                    status: file.index,
                })
            }
        }
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
                // 2 lines of context above the hunk's new-side start (1-based
                // → 0-based). Clamp so a stale anchor past the current file
                // length doesn't open on a blank region.
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

    // Mirrors the gates in `toggle_diff_file_view` so the hint bar only
    // advertises `v: view file` when a press would act.
    pub(crate) fn can_open_file_view(&self) -> bool {
        self.mode != ViewMode::Tree && self.current_file_view_key().is_some()
    }

    pub fn toggle_diff_file_view(&mut self) {
        // Tree mode's right pane is always the raw file preview; `v`/`s` are no-ops.
        if self.mode == ViewMode::Tree {
            return;
        }
        if self.diff.view == DiffPaneView::File {
            self.diff.search.clear();
            self.diff.view = DiffPaneView::Diff;
            return;
        }
        let Some(key) = self.current_file_view_key() else {
            return;
        };
        if self.diff.file_view.key.as_ref() != Some(&key) {
            self.load_file_view(key);
        }
        self.diff.search.clear();
        self.diff.view = DiffPaneView::File;
    }

    pub fn toggle_diff_split_view(&mut self) {
        if self.mode == ViewMode::Tree {
            return;
        }
        self.diff.view = if self.diff.view == DiffPaneView::Split {
            DiffPaneView::Diff
        } else {
            DiffPaneView::Split
        };
    }

    /// Step to the next display: unified → split → file → unified.
    ///
    /// `v` and `s` each toggle one view against the unified default, which
    /// leaves the third one undiscoverable unless you already know it exists.
    /// One key that walks all three makes the set visible; the direct toggles
    /// stay for jumping straight to a known view.
    ///
    /// The file step is skipped when there is nothing to open (no selection, or
    /// a commit whose file cannot be resolved) rather than being a dead press —
    /// the same gate `can_open_file_view` puts on `v`.
    pub fn cycle_diff_view(&mut self) {
        // Tree mode's right pane is always the raw file preview, so there is
        // no cycle to walk — matching `v`/`s`.
        if self.mode == ViewMode::Tree {
            return;
        }
        match self.diff.view {
            DiffPaneView::Diff => self.toggle_diff_split_view(),
            DiffPaneView::Split => {
                self.diff.view = DiffPaneView::Diff;
                if self.can_open_file_view() {
                    self.toggle_diff_file_view();
                }
            }
            DiffPaneView::File => self.toggle_diff_file_view(),
        }
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
            self.raise_notice(NoticeKind::Diff, e.to_string());
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
            self.raise_notice(NoticeKind::Diff, e.to_string());
        }
        self.apply_diff_result(result, DiffApply::ResetWithTitle(&title));
    }
}
