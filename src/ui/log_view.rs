use crate::git::diff::{ChangedFile, CommitEntry};
use crate::ui::SearchQuery;
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
    /// Commit-list incremental search. Mirrors `StatusView::search_query` /
    /// `search_active` / `filter_cache`: the cache contains indices into
    /// `commits` whose summary matches the lowercased query (or `0..len`
    /// when the query is empty). Recomputed only when commits or the query
    /// change.
    pub commit_search_query: SearchQuery,
    pub commit_search_active: bool,
    pub(crate) commits_filter_cache: Vec<usize>,
    /// Drill-down file-list incremental search. Same shape as the commit
    /// search above; indices reference `commit_files`.
    pub file_search_query: SearchQuery,
    pub file_search_active: bool,
    pub(crate) commit_files_filter_cache: Vec<usize>,
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
        self.recompute_commit_filter();
    }

    /// Install a freshly-fetched first page. Resets pagination state via
    /// `set_commits` and computes `fully_loaded` from the page length so the
    /// callsite doesn't have to repeat the short-page sentinel logic.
    pub(crate) fn set_commits_from_first_page(&mut self, page: Vec<CommitEntry>, page_size: usize) {
        let fully_loaded = page.len() < page_size;
        self.set_commits(page);
        self.fully_loaded = fully_loaded;
    }

    /// Append a freshly-fetched page to the tail. `page_size` is the limit
    /// the caller asked for: a short result means we've reached the end.
    pub(crate) fn append_page(&mut self, mut page: Vec<CommitEntry>, page_size: usize) {
        let received = page.len();
        if received > 0 {
            self.commits.append(&mut page);
            self.loaded_count = self.commits.len();
            self.commit_width_cache.set(None);
            self.recompute_commit_filter();
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
        self.recompute_file_filter();
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
        // Drop any file-list search state so a later drill-in starts fresh
        // and does not carry the previous commit's query into the new view.
        self.file_search_active = false;
        self.file_search_query.clear();
        self.commit_files_filter_cache.clear();
    }

    /// Refresh `commits_filter_cache` from `commits` and the current query.
    /// Callers must invoke this after mutating `commits` or
    /// `commit_search_query`; otherwise the cache will diverge from state.
    /// Mirrors `StatusView::recompute_filter`.
    pub(crate) fn recompute_commit_filter(&mut self) {
        self.commits_filter_cache.clear();
        if self.commit_search_query.is_empty() {
            self.commits_filter_cache.extend(0..self.commits.len());
            return;
        }
        let q = self.commit_search_query.lower();
        for (i, c) in self.commits.iter().enumerate() {
            if c.summary_lower.contains(q) {
                self.commits_filter_cache.push(i);
            }
        }
    }

    /// Refresh `commit_files_filter_cache` from `commit_files` and the
    /// current query.
    pub(crate) fn recompute_file_filter(&mut self) {
        self.commit_files_filter_cache.clear();
        if self.file_search_query.is_empty() {
            self.commit_files_filter_cache
                .extend(0..self.commit_files.len());
            return;
        }
        let q = self.file_search_query.lower();
        for (i, f) in self.commit_files.iter().enumerate() {
            if f.path_lower.contains(q) {
                self.commit_files_filter_cache.push(i);
            }
        }
    }

    pub fn start_commit_search(&mut self) {
        self.commit_search_active = true;
    }

    /// Exit the commit-list search bar and clear any active query. Always
    /// recomputes the filter so the caller can refresh the diff against the
    /// now-unfiltered list without inspecting prior state.
    pub fn cancel_commit_search(&mut self) {
        self.commit_search_active = false;
        self.commit_search_query.clear();
        self.recompute_commit_filter();
    }

    /// Hide the commit-list search bar. Returns `true` when the query was
    /// empty and the call therefore collapsed to a cancel (so the caller
    /// knows to refresh the diff for the now-unfiltered list).
    pub fn confirm_commit_search(&mut self) -> bool {
        if self.commit_search_query.is_empty() {
            self.cancel_commit_search();
            true
        } else {
            self.commit_search_active = false;
            false
        }
    }

    pub fn commit_search_push(&mut self, ch: char) {
        self.commit_search_query.push(ch);
        self.recompute_commit_filter();
    }

    pub fn commit_search_pop(&mut self) {
        self.commit_search_query.pop();
        self.recompute_commit_filter();
    }

    pub fn start_file_search(&mut self) {
        self.file_search_active = true;
    }

    pub fn cancel_file_search(&mut self) {
        self.file_search_active = false;
        self.file_search_query.clear();
        self.recompute_file_filter();
    }

    pub fn confirm_file_search(&mut self) -> bool {
        if self.file_search_query.is_empty() {
            self.cancel_file_search();
            true
        } else {
            self.file_search_active = false;
            false
        }
    }

    pub fn file_search_push(&mut self, ch: char) {
        self.file_search_query.push(ch);
        self.recompute_file_filter();
    }

    pub fn file_search_pop(&mut self) {
        self.file_search_query.pop();
        self.recompute_file_filter();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use git2::Oid;

    fn entry(time: i64) -> CommitEntry {
        CommitEntry::new(
            Oid::zero(),
            "deadbee".to_string(),
            format!("c{time}"),
            "T".to_string(),
            time,
        )
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

    fn named_entry(summary: &str) -> CommitEntry {
        CommitEntry::new(
            Oid::zero(),
            "deadbee".to_string(),
            summary.to_string(),
            "T".to_string(),
            0,
        )
    }

    #[test]
    fn commit_filter_empty_query_includes_all_indices() {
        let mut lv = LogView::default();
        lv.set_commits(vec![entry(0), entry(1), entry(2)]);
        assert_eq!(lv.commits_filter_cache, vec![0, 1, 2]);
    }

    #[test]
    fn commit_filter_substring_is_case_insensitive() {
        let mut lv = LogView::default();
        lv.set_commits(vec![
            named_entry("Fix Auth bug"),
            named_entry("feat: AUTH refresh"),
            named_entry("docs: readme"),
        ]);
        lv.commit_search_push('a');
        lv.commit_search_push('u');
        lv.commit_search_push('t');
        lv.commit_search_push('h');
        assert_eq!(lv.commits_filter_cache, vec![0, 1]);
    }

    #[test]
    fn append_page_extends_filter_cache_for_matching_tail() {
        let mut lv = LogView::default();
        lv.set_commits(vec![named_entry("alpha"), named_entry("zulu")]);
        lv.commit_search_push('a');
        assert_eq!(lv.commits_filter_cache, vec![0]);

        lv.append_page(vec![named_entry("quill"), named_entry("apple")], 2);
        assert_eq!(lv.commits_filter_cache, vec![0, 3]);
    }

    #[test]
    fn cancel_commit_search_clears_query_and_resets_cache() {
        let mut lv = LogView::default();
        lv.set_commits(vec![named_entry("alpha"), named_entry("zulu")]);
        lv.start_commit_search();
        lv.commit_search_push('a');
        assert_eq!(lv.commits_filter_cache, vec![0]);

        lv.cancel_commit_search();
        assert!(!lv.commit_search_active);
        assert!(lv.commit_search_query.is_empty());
        assert_eq!(lv.commits_filter_cache, vec![0, 1]);
    }

    #[test]
    fn confirm_commit_search_hides_bar_but_keeps_filter() {
        let mut lv = LogView::default();
        lv.set_commits(vec![named_entry("alpha"), named_entry("zulu")]);
        lv.start_commit_search();
        lv.commit_search_push('a');

        let collapsed_to_cancel = lv.confirm_commit_search();
        assert!(!collapsed_to_cancel);
        assert!(!lv.commit_search_active);
        assert_eq!(lv.commit_search_query.as_str(), "a");
        assert_eq!(lv.commits_filter_cache, vec![0]);
    }

    #[test]
    fn confirm_commit_search_on_empty_query_collapses_to_cancel() {
        let mut lv = LogView::default();
        lv.set_commits(vec![named_entry("alpha")]);
        lv.start_commit_search();

        let collapsed_to_cancel = lv.confirm_commit_search();
        assert!(collapsed_to_cancel);
        assert!(!lv.commit_search_active);
    }

    #[test]
    fn set_commit_files_seeds_filter_cache_under_active_query() {
        let mut lv = LogView::default();
        lv.file_search_push('r');
        lv.set_commit_files(vec![
            ChangedFile::new("readme.md".into(), crate::git::diff::ChangeStatus::Modified),
            ChangedFile::new(
                "src/lib.rs".into(),
                crate::git::diff::ChangeStatus::Modified,
            ),
        ]);
        assert_eq!(lv.commit_files_filter_cache, vec![0, 1]);

        lv.file_search_push('e');
        lv.file_search_push('a');
        // "readme.md" contains "rea"; "src/lib.rs" does not.
        assert_eq!(lv.commit_files_filter_cache, vec![0]);
    }

    #[test]
    fn reset_drill_down_clears_file_search_state() {
        let mut lv = LogView::default();
        lv.drill_down = true;
        lv.set_commit_files(vec![ChangedFile::new(
            "readme.md".into(),
            crate::git::diff::ChangeStatus::Modified,
        )]);
        lv.start_file_search();
        lv.file_search_push('r');
        assert!(lv.file_search_active);
        assert_eq!(lv.commit_files_filter_cache, vec![0]);

        lv.reset_drill_down();
        assert!(!lv.drill_down);
        assert!(!lv.file_search_active);
        assert!(lv.file_search_query.is_empty());
        assert!(lv.commit_files_filter_cache.is_empty());
    }
}
