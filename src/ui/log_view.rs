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

#[cfg(test)]
mod tests {
    use super::*;
    use git2::Oid;

    fn entry(time: i64) -> CommitEntry {
        CommitEntry {
            oid: Oid::zero(),
            short_id: "deadbee".to_string(),
            summary: format!("c{time}"),
            author: "T".to_string(),
            time,
        }
    }

    #[test]
    fn append_page_extends_commits_and_tracks_loaded_count() {
        let mut lv = LogView::default();
        lv.set_commits(vec![entry(0), entry(1)]);

        lv.append_page(vec![entry(2), entry(3)], 2);

        assert_eq!(lv.commits.len(), 4);
        assert_eq!(lv.loaded_count, 4);
        assert!(!lv.fully_loaded);
        assert!(!lv.pending_fetch);
    }

    #[test]
    fn append_page_short_result_marks_fully_loaded() {
        let mut lv = LogView::default();
        lv.set_commits(vec![entry(0), entry(1)]);

        lv.append_page(vec![entry(2)], 3);

        assert_eq!(lv.commits.len(), 3);
        assert!(lv.fully_loaded);
    }

    #[test]
    fn append_page_empty_result_marks_fully_loaded_without_extending() {
        let mut lv = LogView::default();
        lv.set_commits(vec![entry(0)]);

        lv.append_page(Vec::new(), 3);

        assert_eq!(lv.commits.len(), 1);
        assert_eq!(lv.loaded_count, 1);
        assert!(lv.fully_loaded);
        assert!(!lv.pending_fetch);
    }

    #[test]
    fn mark_pending_is_idempotent() {
        let mut lv = LogView::default();
        assert!(lv.mark_pending());
        assert!(!lv.mark_pending());
        lv.clear_pending();
        assert!(lv.mark_pending());
    }

    #[test]
    fn set_commits_resets_pagination_state() {
        let mut lv = LogView::default();
        lv.set_commits(vec![entry(0)]);
        lv.append_page(vec![entry(1)], 5);
        assert!(lv.fully_loaded);

        lv.set_commits(vec![entry(2), entry(3)]);
        assert_eq!(lv.loaded_count, 2);
        assert!(!lv.fully_loaded);
        assert!(!lv.pending_fetch);
    }
}
