//! `App` methods for the read-only file-tree navigator (`ViewMode::Tree`).
//!
//! Directory I/O is synchronous and performed here on the UI thread (one level
//! per expansion via `crate::git::tree::read_children`); the git-status
//! snapshot worker is never involved. Selecting a file row loads its raw
//! contents into the existing file-view pane (`DiffPaneView::File`) — the same
//! surface used by the status/commit file preview, so no new render path is
//! introduced.

use super::{App, DiffPaneView, FileViewKey, FileViewState, LIST_PAGE_SIZE, ViewMode};
use crate::ui::tree_view::{TreeIndexEntry, parent_path};

impl App {
    /// Enter Tree mode: load the root level, clamp the cursor, and preview the
    /// selected row. Safe to call repeatedly (the root read is cached).
    pub fn enter_tree_mode(&mut self) {
        self.mode = ViewMode::Tree;
        // A commit-log page fetch spawned in Log mode could still be in flight;
        // its reply loads a commit diff over `self.diff`, which would clobber
        // the Tree file preview a tick later. Cancel it on entry so only Tree
        // controls the diff pane while this mode is active.
        self.cancel_commit_log_page_fetch();
        // Drop a lingering status-search overlay so its modal key handler can't
        // keep capturing keystrokes after the mode switch. (`clear_diff_state`
        // below clears the diff-search overlay.) This closes the case where a
        // search started before a pending session restore would otherwise route
        // Tree keys into the hidden search handler.
        self.status_view.cancel_search();
        // A prior Tree session could be re-entered with a stale search overlay;
        // start clean so the first keystroke is plain navigation.
        self.tree_view.cancel_search();
        // Drop any diff/file-view state from the prior mode so the right pane
        // starts clean; `preview_tree_selected` repopulates it.
        self.clear_diff_state();
        self.ensure_tree_root();
        let row_count = self.tree_view.visible_rows().len();
        self.tree_view.clamp_selection(row_count);
        self.preview_tree_selected();
    }

    /// Leave Tree mode back to the working-tree status view.
    pub fn exit_tree_to_status(&mut self) {
        self.tree_view.cancel_search();
        self.mode = ViewMode::Status;
        self.clear_diff_state();
        self.refresh_diff(true);
    }

    /// Ensure the root directory's children are loaded into the cache.
    pub(crate) fn ensure_tree_root(&mut self) {
        self.ensure_tree_children("");
    }

    /// Load the immediate children of `dir` (repo-relative) into the cache if
    /// not already present. A read error caches an empty listing and surfaces
    /// the message in the status bar so a single unreadable directory cannot
    /// wedge navigation.
    pub(crate) fn ensure_tree_children(&mut self, dir: &str) {
        if self.tree_view.cache.contains_key(dir) {
            return;
        }
        let respect = self.cfg_tree.respect_gitignore;
        let dir_owned = dir.to_string();
        let result = self.with_repo(|repo| {
            let workdir = repo
                .workdir()
                .ok_or_else(|| anyhow::anyhow!("bare repository has no working tree"))?;
            crate::git::tree::read_children(repo, workdir, &dir_owned, respect)
        });
        match result {
            Ok(children) => {
                self.tree_view.cache.insert(dir.to_string(), children);
            }
            Err(e) => {
                tracing::warn!(error = %e, dir = %dir, "failed to read tree directory");
                self.status = Some(format!("tree error: {e}"));
                // Cache an empty listing so we don't retry the failing read on
                // every keystroke; a repo change / refresh clears the cache.
                self.tree_view.cache.insert(dir.to_string(), Vec::new());
            }
        }
    }

    /// Load the raw contents of the selected file row into the file-view pane.
    /// Directory rows (and an empty tree) show a blank pane — there is no diff
    /// in this mode, so the right pane is always the file overlay.
    pub(crate) fn preview_tree_selected(&mut self) {
        let selected = self.tree_view.selected;
        let row = self.tree_view.visible_rows().into_iter().nth(selected);
        match row {
            Some(row) if !row.is_dir => {
                let key = FileViewKey::Status(row.path);
                if self.diff.file_view.key.as_ref() != Some(&key) {
                    self.load_file_view(key);
                }
                self.diff.view = DiffPaneView::File;
            }
            _ => {
                self.diff.view = DiffPaneView::File;
                self.diff.file_view = FileViewState::default();
            }
        }
    }

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
