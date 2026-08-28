use crate::git::diff::CommitEntry;
use crate::ui::SearchQuery;
use std::cell::Cell;

#[derive(Default)]
pub struct CommitListState {
    pub commits: Vec<CommitEntry>,
    pub selected: usize,
    pub commit_scroll_x: usize,
    pub commit_search_query: SearchQuery,
    pub commit_search_active: bool,
    pub(crate) commits_filter_cache: Vec<usize>,
    pub(crate) commit_width_cache: Cell<Option<(usize, usize)>>,
    pub(crate) loaded_count: usize,
    pub(crate) pending_fetch: bool,
    pub(crate) fully_loaded: bool,
    pub(crate) drill: super::CommitDrillDownState,
}

impl CommitListState {
    pub(crate) fn replace(&mut self, commits: Vec<CommitEntry>) {
        self.loaded_count = commits.len();
        self.commits = commits;
        self.commit_width_cache.set(None);
        self.pending_fetch = false;
        self.fully_loaded = false;
        self.recompute_filter();
    }

    pub(crate) fn replace_first_page(&mut self, page: Vec<CommitEntry>, page_size: usize) {
        let fully_loaded = page.len() < page_size;
        self.replace(page);
        self.fully_loaded = fully_loaded;
    }

    pub(crate) fn append_page(&mut self, mut page: Vec<CommitEntry>, page_size: usize) {
        let received = page.len();
        self.commits.append(&mut page);
        self.loaded_count = self.commits.len();
        if received > 0 {
            self.commit_width_cache.set(None);
            self.recompute_filter();
        }
        self.pending_fetch = false;
        self.fully_loaded |= received < page_size;
    }

    pub(crate) fn mark_pending(&mut self) -> bool {
        if self.pending_fetch {
            false
        } else {
            self.pending_fetch = true;
            true
        }
    }
    pub(crate) fn clear_pending(&mut self) {
        self.pending_fetch = false;
    }
    pub(crate) fn recompute_filter(&mut self) {
        self.commits_filter_cache.clear();
        if self.commit_search_query.is_empty() {
            self.commits_filter_cache.extend(0..self.commits.len());
        } else {
            let query = self.commit_search_query.lower();
            self.commits_filter_cache
                .extend(
                    self.commits
                        .iter()
                        .enumerate()
                        .filter_map(|(index, commit)| {
                            commit.summary_lower.contains(query).then_some(index)
                        }),
                );
        }
    }
}

impl std::ops::Deref for CommitListState {
    type Target = super::CommitDrillDownState;
    fn deref(&self) -> &Self::Target {
        &self.drill
    }
}

impl std::ops::DerefMut for CommitListState {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.drill
    }
}
