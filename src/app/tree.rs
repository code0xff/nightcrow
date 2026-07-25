//! `App` methods for the read-only file-tree navigator (`ViewMode::Tree`).
//!
//! Directory I/O is synchronous on the UI thread (one level per expansion);
//! the git-status snapshot worker is never involved. Selecting a file row
//! loads its raw contents into the existing file-view pane.

use super::{App, DiffPaneView, FileViewKey, FileViewState, NoticeKind, ViewMode};
use std::collections::BTreeSet;

impl App {
    pub fn enter_tree_mode(&mut self) {
        self.mode = ViewMode::Tree;
        // A Log-mode page fetch in flight would clobber the Tree preview a tick
        // later; cancel so only Tree controls the diff pane while active.
        self.cancel_commit_log_page_fetch();
        // Drop lingering search overlays so their modal handlers can't capture
        // Tree keystrokes after the mode switch.
        self.status_view.cancel_search();
        self.tree_view.cancel_search();
        self.clear_diff_state();
        // Re-read from disk so structural changes while away from Tree show up
        // (the per-directory cache is otherwise only cleared on repo switch).
        self.refresh_tree_preserving_cursor();
    }

    pub(crate) fn refresh_tree_preserving_cursor(&mut self) {
        self.refresh_tree_preserving_cursor_scoped(None);
    }

    pub(crate) fn refresh_tree_preserving_cursor_scoped(
        &mut self,
        invalidate: Option<&BTreeSet<String>>,
    ) {
        let prev_path = self.tree_view.selected_path();
        self.refresh_tree_cache_scoped(invalidate);
        // The filtered view renders from the search index, not the cache, so
        // refreshing only the cache would leave results stale until the query
        // changed. The rebuild walks the cache (only invalidated listings
        // re-read), so cost is proportional to what changed.
        if self.tree_view.search_active {
            self.build_tree_index();
            self.tree_view.recompute_filter();
        }
        let rows = self.tree_view.visible_rows();
        if let Some(idx) = prev_path
            .as_deref()
            .and_then(|p| rows.iter().position(|r| r.path == p))
        {
            self.tree_view.selected = idx;
        }
        self.tree_view.clamp_selection(rows.len());
        self.preview_tree_selected();
    }

    pub(crate) fn refresh_tree_cache(&mut self) {
        self.refresh_tree_cache_scoped(None);
    }

    // Scoped form is what a watcher event uses: everything else stays cached,
    // so the rebuild touches disk only for changed directories. The
    // expansion-pruning loop is identical either way — `ensure_tree_children`
    // is a no-op for a listing still in the cache.
    pub(crate) fn refresh_tree_cache_scoped(&mut self, invalidate: Option<&BTreeSet<String>>) {
        match invalidate {
            None => self.tree_view.cache.clear(),
            Some(dirs) => {
                for dir in dirs {
                    self.tree_view.cache.remove(dir);
                }
            }
        }
        self.ensure_tree_root();
        let mut dirs: Vec<String> = self.tree_view.expanded.iter().cloned().collect();
        dirs.sort_by_key(|p| p.matches('/').count());
        let mut kept = BTreeSet::new();
        for dir in dirs {
            let parent = crate::ui::tree_view::parent_path(&dir).unwrap_or("");
            let name = dir.rsplit('/').next().unwrap_or(&dir);
            let still_a_dir = self
                .tree_view
                .cache
                .get(parent)
                .is_some_and(|children| children.iter().any(|e| e.is_dir && e.name == name));
            if still_a_dir {
                self.ensure_tree_children(&dir);
                kept.insert(dir);
            }
        }
        self.tree_view.expanded = kept;
        self.tree_view.row_width_cache.set(None);
        self.sync_tree_watches();
    }

    pub(crate) fn sync_tree_watches(&mut self) {
        if !self.cfg_tree.live_watch {
            return;
        }
        // A filename search matches the whole tree, not just what is expanded,
        // so a file created in a collapsed directory must produce an event.
        let mut desired: BTreeSet<String> = if self.tree_view.search_active {
            self.tree_view.cache.keys().cloned().collect()
        } else {
            self.tree_view.expanded.iter().cloned().collect()
        };
        // Root is always watched so top-level creations/removals are caught
        // even with nothing expanded.
        desired.insert(String::new());
        if let Some(workdir) = self.tree_workdir() {
            self.tree_watch.sync(&workdir, &desired);
        }
    }

    pub(crate) fn clear_tree_watches(&mut self) {
        if let Some(workdir) = self.tree_workdir() {
            self.tree_watch.sync(&workdir, &BTreeSet::new());
        }
    }

    fn tree_workdir(&mut self) -> Option<std::path::PathBuf> {
        self.with_repo(|repo| Ok(repo.workdir().map(|w| w.to_path_buf())))
            .ok()
            .flatten()
    }

    // Cheap half: no directory reread, no preview. Every project runs this each
    // tick so OS events can't pile up behind a hidden tab; rereading waits for
    // that tab to come forward.
    pub fn drain_tree_watcher(&mut self) {
        let changes = self.tree_watch.drain_changed();
        if changes.is_empty() {
            return;
        }
        if changes.unknown {
            // Events may have been dropped — no directory set can be trusted
            // complete; fall back to re-reading everything.
            self.tree_dirty_all = true;
        }
        self.tree_dirty.extend(changes.dirs);
    }

    // Only the project on screen does this — several repos rereading per tick
    // would stall the active tab.
    pub fn poll_tree_watcher(&mut self) {
        self.drain_tree_watcher();
        if self.mode != ViewMode::Tree || (self.tree_dirty.is_empty() && !self.tree_dirty_all) {
            return;
        }
        let all = std::mem::take(&mut self.tree_dirty_all);
        let dirs = std::mem::take(&mut self.tree_dirty);
        self.refresh_tree_preserving_cursor_scoped(if all { None } else { Some(&dirs) });
    }

    pub fn exit_tree_to_status(&mut self) {
        self.tree_view.cancel_search();
        self.clear_tree_watches();
        self.mode = ViewMode::Status;
        self.clear_diff_state();
        self.refresh_diff(true);
    }

    pub(crate) fn ensure_tree_root(&mut self) {
        self.ensure_tree_children("");
    }

    // A read error caches an empty listing and surfaces the message so a
    // single unreadable directory can't wedge navigation.
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
                // A successful read resolves whatever the last failing one
                // reported; without this the tree error outlived its cause.
                self.clear_notice(NoticeKind::Tree);
                self.tree_view.cache.insert(dir.to_string(), children);
            }
            Err(e) => {
                tracing::warn!(error = %e, dir = %dir, "failed to read tree directory");
                self.raise_notice(NoticeKind::Tree, e.to_string());
                // Cache empty so we don't retry the failing read on every
                // keystroke; a repo change / refresh clears the cache.
                self.tree_view.cache.insert(dir.to_string(), Vec::new());
            }
        }
    }

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
}