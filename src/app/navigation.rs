use super::{
    App, ChangedFile, DIFF_PAGE_SIZE, DiffPaneView, Focus, LIST_PAGE_SIZE, ViewMode, cursor_down,
    cursor_up,
};
use crate::git::diff::load_commit_files;
use std::cell::Cell;

impl App {
    pub(crate) fn restore_selection(&mut self, previous_path: Option<&str>) -> Option<String> {
        if self.status_view.files.is_empty() {
            self.status_view.selected = 0;
            return None;
        }

        if let Some(path) = previous_path
            && let Some(index) = self
                .status_view
                .files
                .iter()
                .position(|file| file.path == path)
        {
            self.status_view.selected = index;
            return Some(path.to_string());
        }

        self.status_view.selected = self
            .status_view
            .selected
            .min(self.status_view.files.len().saturating_sub(1));
        self.status_view
            .files
            .get(self.status_view.selected)
            .map(|file| file.path.clone())
    }

    pub fn filtered_indices(&self) -> &[usize] {
        &self.status_view.filter_cache
    }

    pub fn start_search(&mut self) {
        self.status_view.start_search();
    }

    pub fn cancel_search(&mut self) {
        self.status_view.cancel_search();
        self.refresh_status_diff_after_filter_change();
    }

    pub fn confirm_search(&mut self) {
        if self.status_view.confirm_search() {
            self.refresh_status_diff_after_filter_change();
        }
    }

    pub fn search_push(&mut self, ch: char) {
        self.status_view.search_push(ch);
        self.refresh_status_diff_after_filter_change();
    }

    pub fn search_pop(&mut self) {
        self.status_view.search_pop();
        self.refresh_status_diff_after_filter_change();
    }

    pub fn file_scroll_left(&mut self) {
        let target = self.upper_scroll_x_mut();
        *target = target.saturating_sub(4);
    }

    pub fn file_scroll_right(&mut self) {
        let max = self.upper_scroll_x_max();
        let target = self.upper_scroll_x_mut();
        *target = target.saturating_add(4).min(max);
    }

    fn upper_scroll_x_mut(&mut self) -> &mut usize {
        match self.mode {
            ViewMode::Status => &mut self.status_view.file_scroll_x,
            ViewMode::Log if self.log_view.drill_down => &mut self.log_view.file_scroll_x,
            ViewMode::Log => &mut self.log_view.commit_scroll_x,
        }
    }

    fn upper_scroll_x_max(&self) -> usize {
        // Cap at the longest visible entry's char width so we don't drift past
        // the last column of any rendered row. Each branch consults a
        // length-keyed `Cell` cache so repeated keystrokes don't re-walk the
        // full list (and re-count chars per item) every press.
        fn cached_max<'a, T: 'a>(
            cache: &Cell<Option<(usize, usize)>>,
            items: &'a [T],
            width_of: impl Fn(&'a T) -> usize,
        ) -> usize {
            let len = items.len();
            if let Some((cached_len, cached_max)) = cache.get()
                && cached_len == len
            {
                return cached_max;
            }
            let max = items.iter().map(width_of).max().unwrap_or(0);
            cache.set(Some((len, max)));
            max
        }
        match self.mode {
            ViewMode::Status => cached_max(
                &self.status_view.path_width_cache,
                &self.status_view.files,
                |f| f.path.chars().count(),
            ),
            ViewMode::Log if self.log_view.drill_down => cached_max(
                &self.log_view.commit_files_width_cache,
                &self.log_view.commit_files,
                |f| f.path.chars().count(),
            ),
            ViewMode::Log => cached_max(
                &self.log_view.commit_width_cache,
                &self.log_view.commits,
                |c| c.summary.chars().count(),
            ),
        }
    }

    pub(crate) fn selected_filtered_status_path(&self) -> Option<String> {
        self.selected_filtered_status_file().map(|f| f.path.clone())
    }

    /// Borrow-only counterpart of `selected_filtered_status_path` so callers
    /// that just need to read the path don't pay for an allocation. Uses
    /// `binary_search` since `filter_cache` is built in ascending order by
    /// `recompute_filter`.
    pub fn selected_filtered_status_file(&self) -> Option<&ChangedFile> {
        if self
            .filtered_indices()
            .binary_search(&self.status_view.selected)
            .is_err()
        {
            return None;
        }
        self.status_view.files.get(self.status_view.selected)
    }

    pub(crate) fn sync_selection_to_filter(&mut self) -> bool {
        let target = {
            let indices = self.filtered_indices();
            if indices.is_empty() {
                return false;
            }
            if indices.contains(&self.status_view.selected) {
                self.status_view.selected
            } else {
                indices[0]
            }
        };

        if target == self.status_view.selected {
            false
        } else {
            self.status_view.selected = target;
            true
        }
    }

    fn refresh_status_diff_after_filter_change(&mut self) {
        let selection_changed = self.sync_selection_to_filter();
        if self.selected_filtered_status_path().is_none() {
            self.clear_diff_state();
        } else if selection_changed || self.diff.hunks.is_empty() {
            self.reload_diff();
        }
    }

    /// Dispatches a navigation action to the appropriate log list (commit or file).
    /// Returns `true` if the action was handled (i.e. we are in Log mode).
    fn navigate_log_list(&mut self, commit_nav: fn(&mut Self), file_nav: fn(&mut Self)) -> bool {
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

    /// Move `selected` by `delta` positions within the active filter view.
    /// Handles both empty-query (full file list) and non-empty (filtered subset)
    /// cases uniformly.
    pub(crate) fn move_selected_in_filter(&mut self, delta: isize) {
        // Resolve the new selection in a scoped block so the borrow on
        // filtered_indices does not outlive the mutating reload below.
        let resolved = {
            let indices = self.filtered_indices();
            if indices.is_empty() {
                None
            } else {
                let pos = indices.iter().position(|&i| i == self.status_view.selected);
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
            && (Some(new_pos) != pos || self.status_view.selected != new_selected)
        {
            // Mark only after confirming the selection actually changed so
            // that bumping against either end of the list doesn't reset the
            // auto-follow steered-path memory.
            self.mark_user_navigated();
            self.status_view.selected = new_selected;
            self.status_view.file_scroll_x = 0;
            self.reload_diff();
        }
    }

    // ── Selection navigation (status + log shared) ────────────────

    pub fn select_up(&mut self) {
        match self.focus {
            Focus::FileList => {
                if self.navigate_log_list(Self::log_select_up, Self::log_file_select_up) {
                    return;
                }
                self.move_selected_in_filter(-1);
            }
            Focus::DiffViewer => {
                if self.diff.view == DiffPaneView::File {
                    self.diff.file_view.scroll_up(1);
                } else {
                    self.diff.scroll = self.diff.scroll.saturating_sub(1);
                }
            }
            Focus::Terminal => {}
        }
    }

    pub fn select_down(&mut self) {
        match self.focus {
            Focus::FileList => {
                if self.navigate_log_list(Self::log_select_down, Self::log_file_select_down) {
                    return;
                }
                self.move_selected_in_filter(1);
            }
            Focus::DiffViewer => {
                if self.diff.view == DiffPaneView::File {
                    self.diff.file_view.scroll_down(1);
                } else {
                    self.diff.scroll = self
                        .diff
                        .scroll
                        .saturating_add(1)
                        .min(self.diff.max_scroll());
                }
            }
            Focus::Terminal => {}
        }
    }

    pub fn page_up(&mut self) {
        match self.focus {
            Focus::FileList => {
                if self.navigate_log_list(Self::log_page_up, Self::log_file_page_up) {
                    return;
                }
                self.move_selected_in_filter(-(LIST_PAGE_SIZE as isize));
            }
            Focus::DiffViewer => {
                if self.diff.view == DiffPaneView::File {
                    self.diff.file_view.scroll_up(DIFF_PAGE_SIZE);
                } else {
                    self.diff.scroll = self.diff.scroll.saturating_sub(DIFF_PAGE_SIZE);
                }
            }
            Focus::Terminal => {}
        }
    }

    pub fn page_down(&mut self) {
        match self.focus {
            Focus::FileList => {
                if self.navigate_log_list(Self::log_page_down, Self::log_file_page_down) {
                    return;
                }
                self.move_selected_in_filter(LIST_PAGE_SIZE as isize);
            }
            Focus::DiffViewer => {
                if self.diff.view == DiffPaneView::File {
                    self.diff.file_view.scroll_down(DIFF_PAGE_SIZE);
                } else {
                    self.diff.scroll = self
                        .diff
                        .scroll
                        .saturating_add(DIFF_PAGE_SIZE)
                        .min(self.diff.max_scroll());
                }
            }
            Focus::Terminal => {}
        }
    }

    // ── Log view ──────────────────────────────────────────────────

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
        if self.log_view.file_select_up(1) {
            self.load_file_diff_for_log_file_selected();
        }
    }

    pub fn log_file_select_down(&mut self) {
        if self.log_view.file_select_down(1) {
            self.load_file_diff_for_log_file_selected();
        }
    }

    pub fn log_file_page_up(&mut self) {
        if self.log_view.file_select_up(LIST_PAGE_SIZE) {
            self.load_file_diff_for_log_file_selected();
        }
    }

    pub fn log_file_page_down(&mut self) {
        if self.log_view.file_select_down(LIST_PAGE_SIZE) {
            self.load_file_diff_for_log_file_selected();
        }
    }

    pub fn log_select_up(&mut self) {
        if cursor_up(&mut self.log_view.selected, 1) {
            self.log_view.commit_scroll_x = 0;
            self.load_commit_diff_for_selected();
        }
    }

    pub fn log_select_down(&mut self) {
        if cursor_down(&mut self.log_view.selected, self.log_view.commits.len(), 1) {
            self.log_view.commit_scroll_x = 0;
            self.load_commit_diff_for_selected();
        }
        self.maybe_prefetch_commit_log();
    }

    pub fn log_page_up(&mut self) {
        if cursor_up(&mut self.log_view.selected, LIST_PAGE_SIZE) {
            self.log_view.commit_scroll_x = 0;
            self.load_commit_diff_for_selected();
        }
    }

    pub fn log_page_down(&mut self) {
        if cursor_down(
            &mut self.log_view.selected,
            self.log_view.commits.len(),
            LIST_PAGE_SIZE,
        ) {
            self.log_view.commit_scroll_x = 0;
            self.load_commit_diff_for_selected();
        }
        self.maybe_prefetch_commit_log();
    }
}
