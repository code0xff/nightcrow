use super::{App, ViewMode};
use std::cell::Cell;

impl App {
    pub fn file_scroll_left(&mut self) {
        let target = self.upper_scroll_x_mut();
        *target = target.saturating_sub(4);
    }

    pub fn file_scroll_right(&mut self) {
        let max = self.upper_scroll_x_max();
        let target = self.upper_scroll_x_mut();
        *target = target.saturating_add(4).min(max);
    }

    pub(crate) fn upper_scroll_x_mut(&mut self) -> &mut usize {
        match self.mode {
            ViewMode::Status => &mut self.status_view.file_scroll_x,
            ViewMode::Tree => &mut self.tree_view.scroll_x,
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
                |f| f.display_path().chars().count(),
            ),
            // Tree rows are derived (not a stored slice), so cache the max by
            // visible-row count directly rather than via `cached_max`. Width =
            // indent (depth*2) + 2-char dir/file marker + name char count.
            ViewMode::Tree => {
                let rows = self.tree_view.visible_rows();
                let len = rows.len();
                if let Some((cached_len, cached_max)) = self.tree_view.row_width_cache.get()
                    && cached_len == len
                {
                    cached_max
                } else {
                    let max = rows
                        .iter()
                        .map(|r| r.depth * 2 + 2 + r.name.chars().count())
                        .max()
                        .unwrap_or(0);
                    self.tree_view.row_width_cache.set(Some((len, max)));
                    max
                }
            }
            ViewMode::Log if self.log_view.drill_down => cached_max(
                &self.log_view.commit_files_width_cache,
                &self.log_view.commit_files,
                |f| f.display_path().chars().count(),
            ),
            ViewMode::Log => cached_max(
                &self.log_view.commit_width_cache,
                &self.log_view.commits,
                |c| c.summary.chars().count(),
            ),
        }
    }
}