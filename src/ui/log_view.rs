use crate::app::{cursor_down, cursor_up};
use crate::git::diff::{ChangedFile, CommitEntry};
use std::cell::Cell;

#[derive(Default)]
pub struct LogView {
    pub commits: Vec<CommitEntry>,
    pub selected: usize,
    pub diff_title: String,
    pub drill_down: bool,
    pub commit_files: Vec<ChangedFile>,
    pub file_selected: usize,
    pub commit_scroll_x: usize,
    pub file_scroll_x: usize,
    /// Memoized longest-summary char width, keyed by `commits.len()`. See
    /// `StatusView::path_width_cache` for the invalidation contract.
    pub(crate) commit_width_cache: Cell<Option<(usize, usize)>>,
    /// Memoized longest-path char width for `commit_files`.
    pub(crate) commit_files_width_cache: Cell<Option<(usize, usize)>>,
    /// Count of commits currently loaded. Maintained in lockstep with
    /// `commits.len()` by the pagination helpers; kept as a discrete field so
    /// the worker channel can compare against an expected `skip` when results
    /// arrive (drop pages produced from a stale view).
    pub(crate) loaded_count: usize,
    /// A background page fetch is in flight. Guards against issuing duplicate
    /// requests for the same tail.
    pub(crate) pending_fetch: bool,
    /// The previous fetch returned fewer entries than requested, so no further
    /// pages exist. Cleared by `reset_pagination`.
    pub(crate) fully_loaded: bool,
}

impl LogView {
    /// Replace `commits` and invalidate the summary-width cache. See
    /// `StatusView::set_files` for the same-length rationale. Also resets
    /// pagination bookkeeping because `commits` is no longer the result of
    /// the previous page sequence.
    pub(crate) fn set_commits(&mut self, commits: Vec<CommitEntry>) {
        self.loaded_count = commits.len();
        self.commits = commits;
        self.commit_width_cache.set(None);
        self.pending_fetch = false;
        self.fully_loaded = false;
    }

    /// Clear pagination state without touching `commits`. Used when the
    /// receiver/worker is being torn down (e.g. repo switch) and the existing
    /// commit list will be replaced separately.
    pub(crate) fn reset_pagination(&mut self) {
        self.loaded_count = self.commits.len();
        self.pending_fetch = false;
        self.fully_loaded = false;
    }

    /// Append a freshly-fetched page to the tail. `page_size` is the limit
    /// the caller asked for: a short result means we've reached the end.
    pub(crate) fn append_page(&mut self, mut page: Vec<CommitEntry>, page_size: usize) {
        let received = page.len();
        if received > 0 {
            self.commits.append(&mut page);
            self.loaded_count = self.commits.len();
            self.commit_width_cache.set(None);
        }
        self.pending_fetch = false;
        if received < page_size {
            self.fully_loaded = true;
        }
    }

    /// Mark a fetch as in flight. Returns `true` if the flag transitioned,
    /// `false` if a fetch was already pending so the caller should not spawn
    /// another worker.
    pub(crate) fn mark_pending(&mut self) -> bool {
        if self.pending_fetch {
            return false;
        }
        self.pending_fetch = true;
        true
    }

    /// Clear the pending flag without appending a page. Used when a worker
    /// result is discarded (stale skip, repo switch).
    pub(crate) fn clear_pending(&mut self) {
        self.pending_fetch = false;
    }

    /// Replace `commit_files` and invalidate the file-width cache so a
    /// same-length drill-in into a different commit doesn't reuse the
    /// previous commit's max path width.
    pub(crate) fn set_commit_files(&mut self, files: Vec<ChangedFile>) {
        self.commit_files = files;
        self.commit_files_width_cache.set(None);
    }

    /// Exit drill-down so the upper pane shows the commit list again. Clears
    /// the file list and resets file-side cursors/scroll so a later drill-in
    /// starts from a clean state.
    pub fn reset_drill_down(&mut self) {
        self.drill_down = false;
        self.commit_files.clear();
        self.commit_files_width_cache.set(None);
        self.file_selected = 0;
        self.file_scroll_x = 0;
    }

    /// Move the file-list cursor up by `n`. Returns whether the selection
    /// actually changed so the caller can decide whether to reload the diff.
    /// A non-zero move also resets `file_scroll_x` to mirror the established
    /// behaviour of clearing horizontal scroll when the highlighted row moves.
    pub fn file_select_up(&mut self, n: usize) -> bool {
        let moved = cursor_up(&mut self.file_selected, n);
        if moved {
            self.file_scroll_x = 0;
        }
        moved
    }

    /// Move the file-list cursor down by `n`. See `file_select_up` for the
    /// return-value contract.
    pub fn file_select_down(&mut self, n: usize) -> bool {
        let moved = cursor_down(&mut self.file_selected, self.commit_files.len(), n);
        if moved {
            self.file_scroll_x = 0;
        }
        moved
    }
}
