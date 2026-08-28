use crate::git::diff::ChangedFile;
use crate::ui::SearchQuery;
use std::cell::Cell;

#[derive(Default)]
pub struct CommitDrillDownState {
    pub drill_down: bool,
    pub commit_files: Vec<ChangedFile>,
    pub file_selected: usize,
    pub file_scroll_x: usize,
    pub file_search_query: SearchQuery,
    pub file_search_active: bool,
    pub(crate) commit_files_filter_cache: Vec<usize>,
    pub(crate) commit_files_width_cache: Cell<Option<(usize, usize)>>,
}

impl CommitDrillDownState {
    pub(crate) fn replace_files(&mut self, files: Vec<ChangedFile>) {
        self.commit_files = files;
        self.commit_files_width_cache.set(None);
        self.recompute_filter();
    }

    pub(crate) fn reset(&mut self) {
        self.drill_down = false;
        self.commit_files.clear();
        self.commit_files_width_cache.set(None);
        self.file_selected = 0;
        self.file_scroll_x = 0;
        self.file_search_active = false;
        self.file_search_query.clear();
        self.commit_files_filter_cache.clear();
    }

    pub(crate) fn recompute_filter(&mut self) {
        self.commit_files_filter_cache.clear();
        if self.file_search_query.is_empty() {
            self.commit_files_filter_cache
                .extend(0..self.commit_files.len());
        } else {
            let query = self.file_search_query.lower();
            self.commit_files_filter_cache.extend(
                self.commit_files
                    .iter()
                    .enumerate()
                    .filter_map(|(index, file)| file.search_lower.contains(query).then_some(index)),
            );
        }
    }
}
