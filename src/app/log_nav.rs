use super::{App, LIST_PAGE_SIZE, ViewMode};

fn resolve_filtered_selection(indices: &[usize], selected: usize, delta: isize) -> Option<usize> {
    if indices.is_empty() {
        return None;
    }
    let position = match indices.iter().position(|&index| index == selected) {
        Some(position) => position,
        None => return Some(indices[0]),
    };
    let last = indices.len() as isize - 1;
    Some(indices[(position as isize).saturating_add(delta).clamp(0, last) as usize])
}

impl App {
    pub fn log_commit_filtered_indices(&self) -> &[usize] {
        &self.git.view.log.commits_filter_cache
    }

    pub fn log_file_filtered_indices(&self) -> &[usize] {
        &self.git.view.log.commit_files_filter_cache
    }

    // Returns whether selection changed so the caller can decide whether to
    // reload the diff.
    fn sync_log_commit_selection_to_filter(&mut self) -> bool {
        let target = match resolve_filtered_selection(
            self.log_commit_filtered_indices(),
            self.git.view.log.selected,
            0,
        ) {
            Some(target) => target,
            None => return false,
        };
        if target == self.git.view.log.selected {
            false
        } else {
            self.git.view.log.selected = target;
            self.git.view.log.commit_scroll_x = 0;
            true
        }
    }

    fn sync_log_file_selection_to_filter(&mut self) -> bool {
        let target = match resolve_filtered_selection(
            self.log_file_filtered_indices(),
            self.git.view.log.file_selected,
            0,
        ) {
            Some(target) => target,
            None => return false,
        };
        if target == self.git.view.log.file_selected {
            false
        } else {
            self.git.view.log.file_selected = target;
            self.git.view.log.file_scroll_x = 0;
            true
        }
    }

    fn refresh_commit_diff_after_filter_change(&mut self) {
        let selection_changed = self.sync_log_commit_selection_to_filter();
        if self.log_commit_filtered_indices().is_empty() {
            self.clear_diff_state();
        } else if selection_changed || self.git.view.diff.hunks().is_empty() {
            self.load_commit_diff_for_selected();
        }
    }

    fn refresh_file_diff_after_filter_change(&mut self) {
        let selection_changed = self.sync_log_file_selection_to_filter();
        if self.log_file_filtered_indices().is_empty() {
            self.clear_diff_state();
        } else if selection_changed || self.git.view.diff.hunks().is_empty() {
            self.load_file_diff_for_log_file_selected();
        }
    }

    pub fn start_log_search(&mut self) {
        if self.git.view.log.drill_down {
            self.git.view.log.start_file_search();
        } else {
            self.git.view.log.start_commit_search();
        }
    }

    pub fn cancel_log_search(&mut self) {
        if self.git.view.log.drill_down {
            self.git.view.log.cancel_file_search();
            self.refresh_file_diff_after_filter_change();
        } else {
            self.git.view.log.cancel_commit_search();
            self.refresh_commit_diff_after_filter_change();
            // Search ended → prefetch may have been pending; resume if the
            // selection now sits near the loaded tail.
            self.maybe_prefetch_commit_log();
        }
    }

    pub fn confirm_log_search(&mut self) {
        if self.git.view.log.drill_down {
            if self.git.view.log.confirm_file_search() {
                self.refresh_file_diff_after_filter_change();
            }
        } else {
            if self.git.view.log.confirm_commit_search() {
                self.refresh_commit_diff_after_filter_change();
            }
            // Resume prefetch regardless of whether the query was empty:
            // confirm hides the search bar in both branches, so the gate in
            // `maybe_prefetch_commit_log` no longer applies.
            self.maybe_prefetch_commit_log();
        }
    }

    pub fn log_search_push(&mut self, ch: char) {
        if self.git.view.log.drill_down {
            self.git.view.log.file_search_push(ch);
            self.refresh_file_diff_after_filter_change();
        } else {
            self.git.view.log.commit_search_push(ch);
            self.refresh_commit_diff_after_filter_change();
        }
    }

    pub fn log_search_pop(&mut self) {
        if self.git.view.log.drill_down {
            self.git.view.log.file_search_pop();
            self.refresh_file_diff_after_filter_change();
        } else {
            self.git.view.log.commit_search_pop();
            self.refresh_commit_diff_after_filter_change();
        }
    }

    // Returns `true` if handled (i.e. we are in Log mode).
    pub(super) fn navigate_log_list(
        &mut self,
        commit_nav: fn(&mut Self),
        file_nav: fn(&mut Self),
    ) -> bool {
        if self.git.view.mode != ViewMode::Log {
            return false;
        }
        if self.git.view.log.drill_down {
            file_nav(self);
        } else {
            commit_nav(self);
        }
        true
    }

    pub fn log_drill_in(&mut self) {
        let (oid, title) = match self.git.view.log.commits.get(self.git.view.log.selected) {
            Some(entry) => (entry.oid, entry.to_string()),
            None => return,
        };
        self.git
            .load_controller
            .request_commit_files(&self.git.repo_path, oid, title);
    }

    pub fn log_drill_out(&mut self) {
        self.git.load_controller.cancel_commit_files();
        self.git.view.log.reset_drill_down();
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
            self.git.view.log.commit_scroll_x = 0;
            self.load_commit_diff_for_selected();
        }
    }

    pub fn log_select_down(&mut self) {
        if self.move_log_commit_in_filter(1) {
            self.git.view.log.commit_scroll_x = 0;
            self.load_commit_diff_for_selected();
        }
        self.maybe_prefetch_commit_log();
    }

    pub fn log_page_up(&mut self) {
        if self.move_log_commit_in_filter(-(LIST_PAGE_SIZE as isize)) {
            self.git.view.log.commit_scroll_x = 0;
            self.load_commit_diff_for_selected();
        }
    }

    pub fn log_page_down(&mut self) {
        if self.move_log_commit_in_filter(LIST_PAGE_SIZE as isize) {
            self.git.view.log.commit_scroll_x = 0;
            self.load_commit_diff_for_selected();
        }
        self.maybe_prefetch_commit_log();
    }

    // Returns whether the selection actually moved so callers can decide to
    // reload the diff.
    pub(crate) fn move_log_commit_in_filter(&mut self, delta: isize) -> bool {
        let resolved = match resolve_filtered_selection(
            self.log_commit_filtered_indices(),
            self.git.view.log.selected,
            delta,
        ) {
            Some(resolved) => resolved,
            None => return false,
        };
        if resolved == self.git.view.log.selected {
            false
        } else {
            self.git.view.log.selected = resolved;
            true
        }
    }

    pub(crate) fn move_log_file_in_filter(&mut self, delta: isize) -> bool {
        let resolved = match resolve_filtered_selection(
            self.log_file_filtered_indices(),
            self.git.view.log.file_selected,
            delta,
        ) {
            Some(resolved) => resolved,
            None => return false,
        };
        if resolved == self.git.view.log.file_selected {
            false
        } else {
            self.git.view.log.file_selected = resolved;
            self.git.view.log.file_scroll_x = 0;
            true
        }
    }
}
