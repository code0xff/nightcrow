use super::{App, LIST_PAGE_SIZE};
use crate::ui::tree_view::{TreeIndexEntry, parent_path};

impl App {
    /// Move the tree cursor by `delta` rows within the visible list, clamping
    /// at both ends, and preview the new row.
    fn move_tree_selection(&mut self, delta: isize) {
        let len = self.tree_view.visible_rows().len();
        if len == 0 {
            self.tree_view.selected = 0;
            return;
        }
        let last = len as isize - 1;
        let current = self.tree_view.selected.min(len - 1) as isize;
        let new = (current + delta).clamp(0, last) as usize;
        if new != self.tree_view.selected {
            self.tree_view.selected = new;
            self.tree_view.scroll_x = 0;
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

    /// Expand the selected directory row (lazily reading its children). No-op
    /// on file rows, already-expanded directories, or expansion past the
    /// configured `max_depth`.
    pub fn tree_expand(&mut self) {
        let selected = self.tree_view.selected;
        let Some(row) = self.tree_view.visible_rows().into_iter().nth(selected) else {
            return;
        };
        if !row.is_dir || self.tree_view.expanded.contains(&row.path) {
            return;
        }
        if row.depth + 1 > self.cfg_tree.max_depth {
            return;
        }
        self.ensure_tree_children(&row.path);
        self.tree_view.expanded.insert(row.path);
        // Visible rows changed: a same-row-count expand/collapse elsewhere
        // could otherwise reuse a stale horizontal-scroll width bound.
        self.tree_view.row_width_cache.set(None);
        // A newly expanded directory becomes visible — start watching it.
        self.sync_tree_watches();
    }

    /// Collapse the selected directory if expanded; otherwise move the cursor
    /// up to its parent directory row (so repeated `Left` walks back out).
    pub fn tree_collapse(&mut self) {
        let rows = self.tree_view.visible_rows();
        let Some(row) = rows.get(self.tree_view.selected) else {
            return;
        };
        if row.is_dir && self.tree_view.expanded.contains(&row.path) {
            let path = row.path.clone();
            // Drop the directory and every descendant from the expanded set so
            // re-expanding it later starts collapsed rather than restoring a
            // deep open subtree the user explicitly closed.
            let prefix = format!("{path}/");
            self.tree_view
                .expanded
                .retain(|p| p != &path && !p.starts_with(&prefix));
            self.tree_view.row_width_cache.set(None);
            // The collapsed subtree is no longer visible — stop watching it.
            self.sync_tree_watches();
            return;
        }
        if let Some(parent) = parent_path(&row.path) {
            let parent = parent.to_string();
            if let Some(idx) = rows.iter().position(|r| r.path == parent) {
                self.tree_view.selected = idx;
                self.tree_view.scroll_x = 0;
                self.preview_tree_selected();
            }
        }
    }

    /// Enter toggles a directory open/closed; on a file row it (re)loads the
    /// preview, mirroring selection behaviour.
    pub fn tree_toggle(&mut self) {
        let selected = self.tree_view.selected;
        let Some(row) = self.tree_view.visible_rows().into_iter().nth(selected) else {
            return;
        };
        if row.is_dir {
            if self.tree_view.expanded.contains(&row.path) {
                self.tree_collapse();
            } else {
                self.tree_expand();
            }
        } else {
            self.preview_tree_selected();
        }
    }

    /// Open the filename-search overlay: walk the whole tree once to build the
    /// search index, then keep showing the (still unfiltered) view until the
    /// user types a query.
    pub fn start_tree_search(&mut self) {
        self.build_tree_index();
        self.tree_view.search_active = true;
        self.tree_view.search_query.clear();
        self.tree_view.recompute_filter();
        // The results now span the whole tree, so the watch set has to as well
        // — a file created in a directory the user never expanded still
        // changes them. `sync_tree_watches` reads `search_active`, so this
        // must come after it is set.
        self.sync_tree_watches();
    }

    pub fn tree_search_push(&mut self, ch: char) {
        self.tree_view.search_query.push(ch);
        self.tree_view.recompute_filter();
        self.reset_tree_selection_after_filter();
    }

    pub fn tree_search_pop(&mut self) {
        self.tree_view.search_query.pop();
        self.tree_view.recompute_filter();
        self.reset_tree_selection_after_filter();
    }

    /// Close the overlay without changing the expansion state; the cursor stays
    /// on whatever row maps into the now-unfiltered view.
    pub fn cancel_tree_search(&mut self) {
        self.tree_view.cancel_search();
        // Back to watching only what is expanded: the wider set existed for
        // the filtered view and would otherwise hold descriptors for the whole
        // tree until Tree mode was left.
        self.sync_tree_watches();
        let row_count = self.tree_view.visible_rows().len();
        self.tree_view.clamp_selection(row_count);
        self.preview_tree_selected();
    }

    /// Confirm the current selection: reveal it in the normal expansion-based
    /// view by expanding all of its ancestor directories, close the overlay,
    /// and move the cursor onto that path. An empty query collapses to a cancel.
    pub fn confirm_tree_search(&mut self) {
        if self.tree_view.search_query.is_empty() {
            self.cancel_tree_search();
            return;
        }
        let target = self.tree_view.selected_path();
        if let Some(path) = &target {
            // Expand every ancestor so the chosen path is visible once
            // filtering ends. The path itself (if a directory) is left
            // collapsed — the user opens it explicitly.
            let mut p = parent_path(path);
            while let Some(parent) = p {
                self.tree_view.expanded.insert(parent.to_string());
                p = parent_path(parent);
            }
        }
        self.tree_view.cancel_search();
        self.sync_tree_watches();
        if let Some(path) = target {
            let rows = self.tree_view.visible_rows();
            if let Some(idx) = rows.iter().position(|r| r.path == path) {
                self.tree_view.selected = idx;
            }
        }
        self.tree_view.scroll_x = 0;
        let row_count = self.tree_view.visible_rows().len();
        self.tree_view.clamp_selection(row_count);
        self.preview_tree_selected();
    }

    /// After a query change the row set shifts, so pin the cursor to the first
    /// *matching* row (skipping the ancestor directories pulled in only to
    /// connect the path) and re-preview it. Falls back to the first row when
    /// nothing matches the basename directly.
    fn reset_tree_selection_after_filter(&mut self) {
        self.tree_view.scroll_x = 0;
        let rows = self.tree_view.visible_rows();
        if rows.is_empty() {
            self.tree_view.selected = 0;
            self.preview_tree_selected();
            return;
        }
        let q = self.tree_view.search_query.lower();
        let first_match = rows
            .iter()
            .position(|r| r.name.to_lowercase().contains(q))
            .unwrap_or(0);
        self.tree_view.selected = first_match;
        self.preview_tree_selected();
    }

    /// Walk the entire tree once (depth-capped at `max_depth`, gitignore applied
    /// via the same guarded reader used for lazy expansion), populating the
    /// per-directory cache and a flat search index. Synchronous on the UI thread
    /// like the per-level reads — one keystroke triggers it, then all filtering
    /// is in-memory.
    pub(crate) fn build_tree_index(&mut self) {
        self.ensure_tree_root();
        let max_depth = self.cfg_tree.max_depth;
        let mut index = Vec::new();
        // (dir, depth-of-its-children): the root's children sit at depth 0.
        let mut stack = vec![(String::new(), 0usize)];
        while let Some((dir, depth)) = stack.pop() {
            self.ensure_tree_children(&dir);
            let children = match self.tree_view.cache.get(&dir) {
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
                // mirroring the expand guard (`depth + 1 > max_depth` blocks).
                if entry.is_dir && depth < max_depth {
                    stack.push((path, depth + 1));
                }
            }
        }
        self.tree_view.index = index;
    }
}