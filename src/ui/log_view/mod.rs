mod drill_down;
mod list;

pub use drill_down::CommitDrillDownState;
pub use list::CommitListState;

#[derive(Default)]
pub struct LogView {
    pub list: CommitListState,
    pub diff_title: String,
}

impl std::ops::Deref for LogView {
    type Target = CommitListState;
    fn deref(&self) -> &Self::Target {
        &self.list
    }
}

impl std::ops::DerefMut for LogView {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.list
    }
}

impl LogView {
    pub(crate) fn set_commits(&mut self, commits: Vec<crate::git::diff::CommitEntry>) {
        self.list.replace(commits);
    }

    pub(crate) fn set_commits_from_first_page(
        &mut self,
        commits: Vec<crate::git::diff::CommitEntry>,
        page_size: usize,
    ) {
        self.list.replace_first_page(commits, page_size);
    }

    pub(crate) fn set_commit_files(&mut self, files: Vec<crate::git::diff::ChangedFile>) {
        self.list.drill.replace_files(files);
    }

    pub(crate) fn recompute_commit_filter(&mut self) {
        self.list.recompute_filter();
    }

    pub fn reset_drill_down(&mut self) {
        self.list.drill.reset();
    }

    #[cfg(test)]
    pub(crate) fn enter_drill_down(&mut self) {
        self.list.drill.drill_down = true;
    }

    pub fn start_commit_search(&mut self) {
        self.list.commit_search_active = true;
    }
    pub fn cancel_commit_search(&mut self) {
        self.list.commit_search_active = false;
        self.list.commit_search_query.clear();
        self.list.recompute_filter();
    }
    pub fn confirm_commit_search(&mut self) -> bool {
        if self.list.commit_search_query.is_empty() {
            self.cancel_commit_search();
            true
        } else {
            self.list.commit_search_active = false;
            false
        }
    }
    pub fn commit_search_push(&mut self, ch: char) {
        self.list.commit_search_query.push(ch);
        self.list.recompute_filter();
    }
    pub fn commit_search_pop(&mut self) {
        self.list.commit_search_query.pop();
        self.list.recompute_filter();
    }

    pub fn start_file_search(&mut self) {
        self.list.drill.file_search_active = true;
    }
    pub fn cancel_file_search(&mut self) {
        self.list.drill.file_search_active = false;
        self.list.drill.file_search_query.clear();
        self.list.drill.recompute_filter();
    }
    pub fn confirm_file_search(&mut self) -> bool {
        if self.list.drill.file_search_query.is_empty() {
            self.cancel_file_search();
            true
        } else {
            self.list.drill.file_search_active = false;
            false
        }
    }
    pub fn file_search_push(&mut self, ch: char) {
        self.list.drill.file_search_query.push(ch);
        self.list.drill.recompute_filter();
    }
    pub fn file_search_pop(&mut self) {
        self.list.drill.file_search_query.pop();
        self.list.drill.recompute_filter();
    }
}

#[cfg(test)]
mod tests;
