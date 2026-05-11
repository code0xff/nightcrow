use crate::git::diff::ChangedFile;
use std::cell::Cell;
use std::collections::HashMap;
use std::time::SystemTime;

#[derive(Default)]
pub struct StatusView {
    pub files: Vec<ChangedFile>,
    pub selected: usize,
    pub file_scroll_x: usize,
    pub search_query: String,
    pub(crate) search_query_lower: String,
    pub search_active: bool,
    /// Indices into `files` that match the current `search_query`.
    /// Recomputed only when `files` or the query changes (see
    /// `App::recompute_status_filter`). Read-only for renderers.
    pub(crate) filter_cache: Vec<usize>,
    /// Per-file mtime observed at the latest snapshot, keyed by `path`.
    /// Used by the agent-aware focus indicator to decide whether a file
    /// is currently "hot" (recently touched). Entries for paths missing
    /// from the latest snapshot are dropped each tick so the map stays
    /// bounded by the working-tree change count.
    pub hot_table: HashMap<String, SystemTime>,
    /// Memoized longest-path char width, keyed by `files.len()`. Used by
    /// `upper_scroll_x_max` so the right-arrow keystroke does not walk every
    /// path on every press. Invalidated on length change; in this app the
    /// snapshot worker replaces `files` wholesale every tick so length-keyed
    /// invalidation is reliable enough for scroll bounds.
    pub(crate) path_width_cache: Cell<Option<(usize, usize)>>,
}

impl StatusView {
    /// Clear the search query and its lowercase cache together so callers
    /// can't accidentally reset only one and leave the cache stale.
    pub fn clear_search(&mut self) {
        self.search_query.clear();
        self.search_query_lower.clear();
    }

    /// Refresh `filter_cache` from `files` and the current query. Callers must
    /// invoke this after mutating `files`, `search_query`, or
    /// `search_query_lower`; otherwise the cache will diverge from state.
    pub(crate) fn recompute_filter(&mut self) {
        self.filter_cache.clear();
        if self.search_query.is_empty() {
            self.filter_cache.extend(0..self.files.len());
            return;
        }
        let q = self.search_query_lower.as_str();
        for (i, f) in self.files.iter().enumerate() {
            if f.path_lower.contains(q) {
                self.filter_cache.push(i);
            }
        }
    }

    pub fn start_search(&mut self) {
        self.search_active = true;
    }

    /// Exit the search bar and clear any active query. Always recomputes the
    /// filter so the caller can refresh the diff against the now-unfiltered
    /// list without inspecting prior state.
    pub fn cancel_search(&mut self) {
        self.search_active = false;
        self.clear_search();
        self.recompute_filter();
    }

    /// Hide the search bar. Returns `true` when the query was empty and the
    /// call therefore collapsed to a cancel (the caller should refresh the
    /// diff in that case so a stale selection from the empty-filter state is
    /// re-pinned).
    pub fn confirm_search(&mut self) -> bool {
        if self.search_query.is_empty() {
            self.cancel_search();
            true
        } else {
            self.search_active = false;
            false
        }
    }

    pub fn search_push(&mut self, ch: char) {
        self.search_query.push(ch);
        self.search_query_lower = self.search_query.to_lowercase();
        self.recompute_filter();
    }

    pub fn search_pop(&mut self) {
        self.search_query.pop();
        self.search_query_lower = self.search_query.to_lowercase();
        self.recompute_filter();
    }
}

#[derive(Default)]
pub struct RepoInput {
    pub active: bool,
    pub buf: String,
}
