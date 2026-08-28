use super::{App, LIST_PAGE_SIZE};
use crate::ui::tree_view::{TreeIndexEntry, parent_path};

impl App {
    fn move_tree_selection(&mut self, delta: isize) {
        let len = self.git.view.tree.visible_rows().len();
        if len == 0 {
            self.git.view.tree.selected = 0;
            return;
        }
        let last = len as isize - 1;
        let current = self.git.view.tree.selected.min(len - 1) as isize;
        let new = (current + delta).clamp(0, last) as usize;
        if new != self.git.view.tree.selected {
            self.git.view.tree.selected = new;
            self.git.view.tree.scroll_x = 0;
            self.preview_tree_selected();
        }
    }

    pub fn tree_select_up(&mut self) {
        self.move_tree_selection(-1);
    }

    pub fn tree_select_down(&mut self) {
        self.move_tree_selection(1);
    }

    pub fn tree_page_up(&mut self) {
        self.move_tree_selection(-(LIST_PAGE_SIZE as isize));
    }

    pub fn tree_page_down(&mut self) {
        self.move_tree_selection(LIST_PAGE_SIZE as isize);
    }

    pub fn tree_expand(&mut self) {
        let selected = self.git.view.tree.selected;
        let Some(row) = self.git.view.tree.visible_rows().into_iter().nth(selected) else {
            return;
        };
        if !row.is_dir || self.git.view.tree.expanded.contains(&row.path) {
            return;
        }
        if row.depth + 1 > self.git.tree_config.max_depth {
            return;
        }
        self.ensure_tree_children(&row.path);
        self.git.view.tree.expanded.insert(row.path);
        // Visible rows changed: drop a stale horizontal-scroll width bound.
        self.git.view.tree.row_width_cache.set(None);
        // A newly expanded directory becomes visible — start watching it.
        self.sync_tree_watches();
    }

    // Collapse the selected directory if expanded; otherwise move the cursor
    // up to its parent directory row (so repeated `Left` walks back out).
    pub fn tree_collapse(&mut self) {
        let rows = self.git.view.tree.visible_rows();
        let Some(row) = rows.get(self.git.view.tree.selected) else {
            return;
        };
        if row.is_dir && self.git.view.tree.expanded.contains(&row.path) {
            let path = row.path.clone();
            // Drop the directory and every descendant so re-expanding later
            // starts collapsed rather than restoring a deep subtree the user
            // explicitly closed.
            let prefix = format!("{path}/");
            self.git
                .view
                .tree
                .expanded
                .retain(|p| p != &path && !p.starts_with(&prefix));
            self.git.view.tree.row_width_cache.set(None);
            // The collapsed subtree is no longer visible — stop watching it.
            self.sync_tree_watches();
            return;
        }
        if let Some(parent) = parent_path(&row.path) {
            let parent = parent.to_string();
            if let Some(idx) = rows.iter().position(|r| r.path == parent) {
                self.git.view.tree.selected = idx;
                self.git.view.tree.scroll_x = 0;
                self.preview_tree_selected();
            }
        }
    }

    // Enter opens the selected file: load it into the file view and zoom the
    // diff pane so reading is the whole screen. Expansion stays on `→`/`←`, so
    // a directory row does nothing here.
    pub fn tree_open_selected(&mut self) {
        let selected = self.git.view.tree.selected;
        let Some(row) = self.git.view.tree.visible_rows().into_iter().nth(selected) else {
            return;
        };
        if row.is_dir {
            return;
        }
        self.preview_tree_selected();
        self.set_diff_fullscreen(true);
    }

    // Walk the whole tree once to build the search index, then keep showing
    // the (still unfiltered) view until the user types a query.
    pub fn start_tree_search(&mut self) {
        self.build_tree_index();
        self.git.view.tree.search_active = true;
        self.git.view.tree.search_query.clear();
        self.git.view.tree.recompute_filter();
        // The results now span the whole tree, so the watch set has to as well
        // — a file created in a directory the user never expanded still
        // changes them. `sync_tree_watches` reads `search_active`, so this
        // must come after it is set.
        self.sync_tree_watches();
    }

    pub fn tree_search_push(&mut self, ch: char) {
        self.git.view.tree.search_query.push(ch);
        self.git.view.tree.recompute_filter();
        self.reset_tree_selection_after_filter();
    }

    pub fn tree_search_pop(&mut self) {
        self.git.view.tree.search_query.pop();
        self.git.view.tree.recompute_filter();
        self.reset_tree_selection_after_filter();
    }

    // Close the overlay without changing the expansion state; the cursor stays
    // on whatever row maps into the now-unfiltered view.
    pub fn cancel_tree_search(&mut self) {
        self.git.view.tree.cancel_search();
        // Back to watching only what is expanded: the wider set existed for
        // the filtered view and would otherwise hold descriptors for the whole
        // tree until Tree mode was left.
        self.sync_tree_watches();
        let row_count = self.git.view.tree.visible_rows().len();
        self.git.view.tree.clamp_selection(row_count);
        self.preview_tree_selected();
    }

    // Reveal the current selection in the normal expansion-based view by
    // expanding all of its ancestor directories, close the overlay, and move
    // the cursor onto that path. An empty query collapses to a cancel.
    pub fn confirm_tree_search(&mut self) {
        if self.git.view.tree.search_query.is_empty() {
            self.cancel_tree_search();
            return;
        }
        let target = self.git.view.tree.selected_path();
        if let Some(path) = &target {
            // Expand every ancestor so the chosen path is visible once
            // filtering ends. The path itself (if a directory) is left
            // collapsed — the user opens it explicitly.
            let mut p = parent_path(path);
            while let Some(parent) = p {
                self.git.view.tree.expanded.insert(parent.to_string());
                p = parent_path(parent);
            }
        }
        self.git.view.tree.cancel_search();
        self.sync_tree_watches();
        if let Some(path) = target {
            let rows = self.git.view.tree.visible_rows();
            if let Some(idx) = rows.iter().position(|r| r.path == path) {
                self.git.view.tree.selected = idx;
            }
        }
        self.git.view.tree.scroll_x = 0;
        let row_count = self.git.view.tree.visible_rows().len();
        self.git.view.tree.clamp_selection(row_count);
        self.preview_tree_selected();
    }

    // After a query change the row set shifts, so pin the cursor to the first
    // *matching* row (skipping ancestor directories pulled in only to connect
    // the path). Falls back to the first row when nothing matches directly.
    fn reset_tree_selection_after_filter(&mut self) {
        self.git.view.tree.scroll_x = 0;
        let rows = self.git.view.tree.visible_rows();
        if rows.is_empty() {
            self.git.view.tree.selected = 0;
            self.preview_tree_selected();
            return;
        }
        let q = self.git.view.tree.search_query.lower();
        let first_match = rows
            .iter()
            .position(|r| r.name.to_lowercase().contains(q))
            .unwrap_or(0);
        self.git.view.tree.selected = first_match;
        self.preview_tree_selected();
    }

    // Synchronous on the UI thread like the per-level reads — one keystroke
    // triggers it, then all filtering is in-memory.
    pub(crate) fn build_tree_index(&mut self) {
        self.ensure_tree_root();
        let max_depth = self.git.tree_config.max_depth;
        let mut index = Vec::new();
        // (dir, depth-of-its-children): the root's children sit at depth 0.
        let mut stack = vec![(String::new(), 0usize)];
        while let Some((dir, depth)) = stack.pop() {
            self.ensure_tree_children(&dir);
            let children = match self.git.view.tree.cache.get(&dir) {
                Some(c) => c.clone(),
                None => continue,
            };
            for entry in children {
                let path = if dir.is_empty() {
                    entry.name.clone()
                } else {
                    format!("{dir}/{}", entry.name)
                };
                index.push(TreeIndexEntry {
                    name_lower: entry.name.to_lowercase(),
                    path: path.clone(),
                });
                // Descend only while the next level stays within max_depth,
                // mirroring the expand guard.
                if entry.is_dir && depth < max_depth {
                    stack.push((path, depth + 1));
                }
            }
        }
        self.git.view.tree.index = index;
    }
}
