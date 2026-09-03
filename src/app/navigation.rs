use super::{App, ChangedFile, DIFF_PAGE_SIZE, DiffPaneView, Focus, LIST_PAGE_SIZE, ViewMode};

impl App {
    pub(crate) fn restore_selection(&mut self, previous_path: Option<&str>) -> Option<String> {
        if self.git.view.status.files.is_empty() {
            self.git.view.status.selected = 0;
            return None;
        }

        if let Some(path) = previous_path
            && let Some(index) = self
                .git
                .view
                .status
                .files
                .iter()
                .position(|file| file.path == path)
        {
            self.git.view.status.selected = index;
            return Some(path.to_string());
        }

        self.git.view.status.selected = self
            .git
            .view
            .status
            .selected
            .min(self.git.view.status.files.len().saturating_sub(1));
        self.git
            .view
            .status
            .files
            .get(self.git.view.status.selected)
            .map(|file| file.path.clone())
    }

    pub fn filtered_indices(&self) -> &[usize] {
        &self.git.view.status.filter_cache
    }

    pub fn start_search(&mut self) {
        self.git.view.status.start_search();
    }

    pub fn cancel_search(&mut self) {
        self.git.view.status.cancel_search();
        self.refresh_status_diff_after_filter_change();
    }

    pub fn confirm_search(&mut self) {
        if self.git.view.status.confirm_search() {
            self.refresh_status_diff_after_filter_change();
        }
    }

    pub fn search_push(&mut self, ch: char) {
        self.git.view.status.search_push(ch);
        self.refresh_status_diff_after_filter_change();
    }

    pub fn search_pop(&mut self) {
        self.git.view.status.search_pop();
        self.refresh_status_diff_after_filter_change();
    }

    pub(crate) fn selected_filtered_status_path(&self) -> Option<String> {
        self.selected_filtered_status_file().map(|f| f.path.clone())
    }

    // Borrow-only counterpart of `selected_filtered_status_path` so callers
    // that just need to read don't allocate. `binary_search` works because
    // `filter_cache` is built in ascending order by `recompute_filter`.
    pub fn selected_filtered_status_file(&self) -> Option<&ChangedFile> {
        if self
            .filtered_indices()
            .binary_search(&self.git.view.status.selected)
            .is_err()
        {
            return None;
        }
        self.git
            .view
            .status
            .files
            .get(self.git.view.status.selected)
    }

    pub(crate) fn sync_selection_to_filter(&mut self) -> bool {
        let target = {
            let indices = self.filtered_indices();
            if indices.is_empty() {
                return false;
            }
            if indices.contains(&self.git.view.status.selected) {
                self.git.view.status.selected
            } else {
                indices[0]
            }
        };

        if target == self.git.view.status.selected {
            false
        } else {
            self.git.view.status.selected = target;
            // Match `move_selected_in_filter`: drop the previous file's
            // horizontal scroll so the newly-shown path starts from column 0.
            self.git.view.status.file_scroll_x = 0;
            true
        }
    }

    fn refresh_status_diff_after_filter_change(&mut self) {
        let selection_changed = self.sync_selection_to_filter();
        if self.selected_filtered_status_path().is_none() {
            self.clear_diff_state();
        } else if selection_changed || self.git.view.diff.hunks().is_empty() {
            self.reload_diff();
        }
    }

    pub(crate) fn move_selected_in_filter(&mut self, delta: isize) {
        // Resolve the new selection in a scoped block so the borrow on
        // filtered_indices does not outlive the mutating reload below.
        let resolved = {
            let indices = self.filtered_indices();
            if indices.is_empty() {
                None
            } else {
                let pos = indices
                    .iter()
                    .position(|&i| i == self.git.view.status.selected);
                let new_pos = match pos {
                    Some(p) => {
                        let last = indices.len() as isize - 1;
                        (p as isize + delta).clamp(0, last) as usize
                    }
                    None => 0,
                };
                Some((pos, new_pos, indices[new_pos]))
            }
        };
        if let Some((pos, new_pos, new_selected)) = resolved
            && (Some(new_pos) != pos || self.git.view.status.selected != new_selected)
        {
            // Mark only after confirming the selection actually changed so
            // bumping against either end doesn't reset the auto-follow
            // steered-path memory.
            self.mark_user_navigated();
            self.git.view.status.selected = new_selected;
            self.git.view.status.file_scroll_x = 0;
            self.reload_diff();
        }
    }

    pub fn select_up(&mut self) {
        match self.focus {
            Focus::FileList => {
                if self.git.view.mode == ViewMode::Tree {
                    self.tree_select_up();
                    return;
                }
                if self.navigate_log_list(Self::log_select_up, Self::log_file_select_up) {
                    return;
                }
                self.move_selected_in_filter(-1);
            }
            Focus::DiffViewer => {
                if self.git.view.diff.view == DiffPaneView::File {
                    self.git.view.diff.file_view.scroll_up(1);
                } else {
                    self.git.view.diff.scroll = self.git.view.diff.scroll.saturating_sub(1);
                }
            }
            Focus::Terminal => {}
        }
    }

    pub fn select_down(&mut self) {
        match self.focus {
            Focus::FileList => {
                if self.git.view.mode == ViewMode::Tree {
                    self.tree_select_down();
                    return;
                }
                if self.navigate_log_list(Self::log_select_down, Self::log_file_select_down) {
                    return;
                }
                self.move_selected_in_filter(1);
            }
            Focus::DiffViewer => {
                if self.git.view.diff.view == DiffPaneView::File {
                    self.git.view.diff.file_view.scroll_down(1);
                } else {
                    self.git.view.diff.scroll = self
                        .git
                        .view
                        .diff
                        .scroll
                        .saturating_add(1)
                        .min(self.git.view.diff.max_scroll());
                }
            }
            Focus::Terminal => {}
        }
    }

    pub fn page_up(&mut self) {
        match self.focus {
            Focus::FileList => {
                if self.git.view.mode == ViewMode::Tree {
                    self.tree_page_up();
                    return;
                }
                if self.navigate_log_list(Self::log_page_up, Self::log_file_page_up) {
                    return;
                }
                self.move_selected_in_filter(-(LIST_PAGE_SIZE as isize));
            }
            Focus::DiffViewer => {
                if self.git.view.diff.view == DiffPaneView::File {
                    self.git.view.diff.file_view.scroll_up(DIFF_PAGE_SIZE);
                } else {
                    self.git.view.diff.scroll =
                        self.git.view.diff.scroll.saturating_sub(DIFF_PAGE_SIZE);
                }
            }
            Focus::Terminal => {}
        }
    }

    pub fn page_down(&mut self) {
        match self.focus {
            Focus::FileList => {
                if self.git.view.mode == ViewMode::Tree {
                    self.tree_page_down();
                    return;
                }
                if self.navigate_log_list(Self::log_page_down, Self::log_file_page_down) {
                    return;
                }
                self.move_selected_in_filter(LIST_PAGE_SIZE as isize);
            }
            Focus::DiffViewer => {
                if self.git.view.diff.view == DiffPaneView::File {
                    self.git.view.diff.file_view.scroll_down(DIFF_PAGE_SIZE);
                } else {
                    self.git.view.diff.scroll = self
                        .git
                        .view
                        .diff
                        .scroll
                        .saturating_add(DIFF_PAGE_SIZE)
                        .min(self.git.view.diff.max_scroll());
                }
            }
            Focus::Terminal => {}
        }
    }
}
