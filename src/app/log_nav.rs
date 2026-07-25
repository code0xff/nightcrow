use super::{App, LIST_PAGE_SIZE, ViewMode};
use crate::git::diff::load_commit_files;

impl App {
    pub fn log_commit_filtered_indices(&self) -> &[usize] {
        &self.log_view.commits_filter_cache
    }

    pub fn log_file_filtered_indices(&self) -> &[usize] {
        &self.log_view.commit_files_filter_cache
    }

    // Returns whether selection changed so the caller can decide whether to
    // reload the diff.
    fn sync_log_commit_selection_to_filter(&mut self) -> bool {
        let target = {
            let indices = self.log_commit_filtered_indices();
            if indices.is_empty() {
                return false;
            }
            if indices.contains(&self.log_view.selected) {
                self.log_view.selected
            } else {
                indices[0]
            }
        };
        if target == self.log_view.selected {
            false
        } else {
            self.log_view.selected = target;
            self.log_view.commit_scroll_x = 0;
            true
        }
    }

    fn sync_log_file_selection_to_filter(&mut self) -> bool {
        let target = {
            let indices = self.log_file_filtered_indices();
            if indices.is_empty() {
                return false;
            }
            if indices.contains(&self.log_view.file_selected) {
                self.log_view.file_selected
            } else {
                indices[0]
            }
        };
        if target == self.log_view.file_selected {
            false
        } else {
            self.log_view.file_selected = target;
            self.log_view.file_scroll_x = 0;
            true
        }
    }

    fn refresh_commit_diff_after_filter_change(&mut self) {
        let selection_changed = self.sync_log_commit_selection_to_filter();
        if self.log_commit_filtered_indices().is_empty() {
            self.clear_diff_state();
        } else if selection_changed || self.diff.hunks.is_empty() {
            self.load_commit_diff_for_selected();
        }
    }

    fn refresh_file_diff_after_filter_change(&mut self) {
        let selection_changed = self.sync_log_file_selection_to_filter();
        if self.log_file_filtered_indices().is_empty() {
            self.clear_diff_state();
        } else if selection_changed || self.diff.hunks.is_empty() {
            self.load_file_diff_for_log_file_selected();
        }
    }

    pub fn start_log_search(&mut self) {
        if self.log_view.drill_down {
            self.log_view.start_file_search();
        } else {
            self.log_view.start_commit_search();
        }
    }

    pub fn cancel_log_search(&mut self) {
        if self.log_view.drill_down {
            self.log_view.cancel_file_search();
            self.refresh_file_diff_after_filter_change();
        } else {
            self.log_view.cancel_commit_search();
            self.refresh_commit_diff_after_filter_change();
            // Search ended → prefetch may have been pending; resume if the
            // selection now sits near the loaded tail.
            self.maybe_prefetch_commit_log();
        }
    }

    pub fn confirm_log_search(&mut self) {
        if self.log_view.drill_down {
            if self.log_view.confirm_file_search() {
                self.refresh_file_diff_after_filter_change();
            }
        } else {
            if self.log_view.confirm_commit_search() {
                self.refresh_commit_diff_after_filter_change();
            }
            // Resume prefetch regardless of whether the query was empty:
            // confirm hides the search bar in both branches, so the gate in
            // `maybe_prefetch_commit_log` no longer applies.
            self.maybe_prefetch_commit_log();
        }
    }

    pub fn log_search_push(&mut self, ch: char) {
        if self.log_view.drill_down {
            self.log_view.file_search_push(ch);
            self.refresh_file_diff_after_filter_change();
        } else {
            self.log_view.commit_search_push(ch);
            self.refresh_commit_diff_after_filter_change();
        }
    }

    pub fn log_search_pop(&mut self) {
        if self.log_view.drill_down {
            self.log_view.file_search_pop();
            self.refresh_file_diff_after_filter_change();
        } else {
            self.log_view.commit_search_pop();
            self.refresh_commit_diff_after_filter_change();
        }
    }

    // Returns `true` if handled (i.e. we are in Log mode).
    pub(super) fn navigate_log_list(&mut self, commit_nav: fn(&mut Self), file_nav: fn(&mut Self)) -> bool {
        if self.mode != ViewMode::Log {
            return false;
        }
        if self.log_view.drill_down {
            file_nav(self);
        } else {
            commit_nav(self);
        }
        true
    }

    pub fn log_drill_in(&mut self) {
        let (oid, title) = match self.log_view.commits.get(self.log_view.selected) {
            Some(entry) => (entry.oid, entry.to_string()),
            None => return,
        };
        match self.with_repo(|repo| load_commit_files(repo, oid)) {
            Ok(files) => {
                self.log_view.set_commit_files(files);
                self.log_view.file_selected = 0;
                self.log_view.drill_down = true;
                if self.log_view.commit_files.is_empty() {
                    self.clear_diff_state();
                    self.log_view.diff_title = title;
                } else {
                    self.load_file_diff_for_log_file_selected();
                }
            }
            Err(e) => {
                tracing::warn!(error = %e, "failed to load commit files");
            }
        }
    }

    pub fn log_drill_out(&mut self) {
        self.log_view.reset_drill_down();
        self.load_commit_diff_for_selected();
    }

    pub fn log_file_select_up(&mut self) {
        if self.move_log_file_in_filter(-1) {
            self.load_file_diff_for_log_file_selected();
        }
    }

    pub fn log_file_select_down(&mut self) {
        if self.move_log_file_in_filter(1) {
            self.load_file_diff_for_log_file_selected();
        }
    }

    pub fn log_file_page_up(&mut self) {
        if self.move_log_file_in_filter(-(LIST_PAGE_SIZE as isize)) {
            self.load_file_diff_for_log_file_selected();
        }
    }

    pub fn log_file_page_down(&mut self) {
        if self.move_log_file_in_filter(LIST_PAGE_SIZE as isize) {
            self.load_file_diff_for_log_file_selected();
        }
    }

    pub fn log_select_up(&mut self) {
        if self.move_log_commit_in_filter(-1) {
            self.log_view.commit_scroll_x = 0;
            self.load_commit_diff_for_selected();
        }
    }

    pub fn log_select_down(&mut self) {
        if self.move_log_commit_in_filter(1) {
            self.log_view.commit_scroll_x = 0;
            self.load_commit_diff_for_selected();
        }
        self.maybe_prefetch_commit_log();
    }

    pub fn log_page_up(&mut self) {
        if self.move_log_commit_in_filter(-(LIST_PAGE_SIZE as isize)) {
            self.log_view.commit_scroll_x = 0;
            self.load_commit_diff_for_selected();
        }
    }

    pub fn log_page_down(&mut self) {
        if self.move_log_commit_in_filter(LIST_PAGE_SIZE as isize) {
            self.log_view.commit_scroll_x = 0;
            self.load_commit_diff_for_selected();
        }
        self.maybe_prefetch_commit_log();
    }

    // Returns whether the selection actually moved so callers can decide to
    // reload the diff.
    pub(crate) fn move_log_commit_in_filter(&mut self, delta: isize) -> bool {
        let resolved = {
            let indices = self.log_commit_filtered_indices();
            if indices.is_empty() {
                return false;
            }
            let pos = indices.iter().position(|&i| i == self.log_view.selected);
            let new_pos = match pos {
                Some(p) => {
                    let last = indices.len() as isize - 1;
                    (p as isize + delta).clamp(0, last) as usize
                }
                None => 0,
            };
            indices[new_pos]
        };
        if resolved == self.log_view.selected {
            false
        } else {
            self.log_view.selected = resolved;
            true
        }
    }

    pub(crate) fn move_log_file_in_filter(&mut self, delta: isize) -> bool {
        let resolved = {
            let indices = self.log_file_filtered_indices();
            if indices.is_empty() {
                return false;
            }
            let pos = indices
                .iter()
                .position(|&i| i == self.log_view.file_selected);
            let new_pos = match pos {
                Some(p) => {
                    let last = indices.len() as isize - 1;
                    (p as isize + delta).clamp(0, last) as usize
                }
                None => 0,
            };
            indices[new_pos]
        };
        if resolved == self.log_view.file_selected {
            false
        } else {
            self.log_view.file_selected = resolved;
            self.log_view.file_scroll_x = 0;
            true
        }
    }
}