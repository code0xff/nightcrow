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
        match self.git.view.mode {
            ViewMode::Status => &mut self.git.view.status.file_scroll_x,
            ViewMode::Tree => &mut self.git.view.tree.scroll_x,
            ViewMode::Log if self.git.view.log.drill_down => &mut self.git.view.log.file_scroll_x,
            ViewMode::Log => &mut self.git.view.log.commit_scroll_x,
        }
    }

    fn upper_scroll_x_max(&self) -> usize {
        // Cap at the longest visible entry's char width. Each branch consults
        // a length-keyed `Cell` cache so repeated keystrokes don't re-walk the
        // full list every press.
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
        match self.git.view.mode {
            ViewMode::Status => cached_max(
                &self.git.view.status.path_width_cache,
                &self.git.view.status.files,
                |f| f.display_path().chars().count(),
            ),
            // Tree rows are derived (not a stored slice), so cache by
            // visible-row count directly. Width = indent (depth*2) + 2-char
            // dir/file marker + name char count.
            ViewMode::Tree => {
                let rows = self.git.view.tree.visible_rows();
                let len = rows.len();
                if let Some((cached_len, cached_max)) = self.git.view.tree.row_width_cache.get()
                    && cached_len == len
                {
                    cached_max
                } else {
                    let max = rows
                        .iter()
                        .map(|r| r.depth * 2 + 2 + r.name.chars().count())
                        .max()
                        .unwrap_or(0);
                    self.git.view.tree.row_width_cache.set(Some((len, max)));
                    max
                }
            }
            ViewMode::Log if self.git.view.log.drill_down => cached_max(
                &self.git.view.log.commit_files_width_cache,
                &self.git.view.log.commit_files,
                |f| f.display_path().chars().count(),
            ),
            ViewMode::Log => cached_max(
                &self.git.view.log.commit_width_cache,
                &self.git.view.log.commits,
                |c| c.summary.chars().count(),
            ),
        }
    }
}
