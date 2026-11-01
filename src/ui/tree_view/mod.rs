//! File-tree navigator state (`ViewMode::Tree`). The visible row list is
//! derived from a cache + expansion set so the two can never drift; all
//! directory I/O lives in `App`, keeping this module pure and unit-testable.

use crate::git::tree::TreeEntry;
use crate::ui::SearchQuery;
use std::cell::{Cell, RefCell};
use std::collections::{BTreeSet, HashMap, HashSet};
use std::sync::Arc;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VisibleRow {
    pub path: String,
    pub name: String,
    pub is_dir: bool,
    pub depth: usize,
    pub expanded: bool,
}

/// Flat filename-search index entry, built when search opens and discarded
/// when it closes.
#[derive(Debug, Clone)]
pub(crate) struct TreeIndexEntry {
    pub path: String,
    pub name_lower: String,
}

/// Memoized visible rows, dropped when the revision or search state moves.
struct RowsCache {
    revision: u64,
    search_active: bool,
    query: String,
    rows: Arc<Vec<VisibleRow>>,
}

#[derive(Default)]
pub struct TreeView {
    pub selected: usize,
    pub scroll_x: usize,
    /// Repo-relative expanded directory paths. The root (`""`) is implicitly
    /// expanded and never stored here.
    expanded: BTreeSet<String>,
    /// Lazily-populated children, keyed by repo-relative directory path.
    /// Absent = unread; empty vec = read and genuinely empty.
    cache: HashMap<String, Vec<TreeEntry>>,
    /// Memoized longest visible-row char width, keyed by row count.
    pub(crate) row_width_cache: Cell<Option<(usize, usize)>>,
    pub search_active: bool,
    pub search_query: SearchQuery,
    /// Flat index of every entry under the root.
    pub(crate) index: Vec<TreeIndexEntry>,
    /// Repo-relative paths to display while filtering: matches plus ancestors.
    show_set: HashSet<String>,
    /// Count of entries matching the current query.
    pub(crate) match_count: usize,
    /// Bumped by every cache/expansion mutation; guards `rows_cache`.
    rows_revision: Cell<u64>,
    rows_cache: RefCell<Option<RowsCache>>,
}

impl TreeView {
    pub(crate) fn cache(&self) -> &HashMap<String, Vec<TreeEntry>> {
        &self.cache
    }

    pub(crate) fn cache_mut(&mut self) -> &mut HashMap<String, Vec<TreeEntry>> {
        self.invalidate_rows();
        &mut self.cache
    }

    pub fn expanded(&self) -> &BTreeSet<String> {
        &self.expanded
    }

    pub(crate) fn expanded_mut(&mut self) -> &mut BTreeSet<String> {
        self.invalidate_rows();
        &mut self.expanded
    }

    pub(crate) fn set_expanded(&mut self, expanded: BTreeSet<String>) {
        self.expanded = expanded;
        self.invalidate_rows();
    }

    fn invalidate_rows(&self) {
        self.rows_revision
            .set(self.rows_revision.get().wrapping_add(1));
    }

    /// An open overlay with an empty query still shows the expansion view so
    /// the tree does not explode before the user types.
    pub fn search_filtering(&self) -> bool {
        self.search_active && !self.search_query.is_empty()
    }

    pub fn cancel_search(&mut self) {
        self.search_active = false;
        self.search_query.clear();
        self.index.clear();
        self.show_set.clear();
        self.match_count = 0;
        self.row_width_cache.set(None);
        self.invalidate_rows();
    }

    /// Each match contributes itself and every ancestor so the filtered view
    /// renders an unbroken path from the root to each hit.
    pub(crate) fn recompute_filter(&mut self) {
        // Collect matches under an immutable borrow first, then mutate the
        // show-set — `index` and `show_set` both borrow `self`, so they can't
        // be touched in the same loop.
        let matches: Vec<String> = {
            let q = self.search_query.lower();
            if q.is_empty() {
                Vec::new()
            } else {
                self.index
                    .iter()
                    .filter(|e| e.name_lower.contains(q))
                    .map(|e| e.path.clone())
                    .collect()
            }
        };
        self.match_count = matches.len();
        self.show_set.clear();
        for path in matches {
            if self.show_set.insert(path.clone()) {
                // Stop at the first ancestor already present — its own
                // ancestors were added on a prior insert.
                let mut p = path.as_str();
                while let Some(parent) = parent_path(p) {
                    if !self.show_set.insert(parent.to_string()) {
                        break;
                    }
                    p = parent;
                }
            }
        }
        self.invalidate_rows();
    }

    /// Visible rows shared with the renderer and navigation. Rebuilt only when
    /// the child cache, expanded set, or search state changes; callers that
    /// run per frame clone the `Arc` instead of every row's strings.
    pub(crate) fn rows(&self) -> Arc<Vec<VisibleRow>> {
        if let Some(cached) = self.rows_cache.borrow().as_ref()
            && cached.revision == self.rows_revision.get()
            && cached.search_active == self.search_active
            && cached.query == self.search_query.as_str()
        {
            return Arc::clone(&cached.rows);
        }
        let rows = Arc::new(self.build_rows());
        *self.rows_cache.borrow_mut() = Some(RowsCache {
            revision: self.rows_revision.get(),
            search_active: self.search_active,
            query: self.search_query.as_str().to_string(),
            rows: Arc::clone(&rows),
        });
        rows
    }

    fn build_rows(&self) -> Vec<VisibleRow> {
        let mut rows = Vec::new();
        if self.search_filtering() {
            self.push_children_filtered("", 0, &mut rows);
        } else {
            self.push_children("", 0, &mut rows);
        }
        rows
    }

    /// Only expanded, cached directories contribute children, so this never
    /// triggers I/O. While filtering, the row list is restricted to `show_set`.
    #[cfg(test)]
    pub fn visible_rows(&self) -> Vec<VisibleRow> {
        (*self.rows()).clone()
    }

    /// Renders every kept directory as expanded so the full path to each
    /// match is visible.
    fn push_children_filtered(&self, dir: &str, depth: usize, rows: &mut Vec<VisibleRow>) {
        let Some(children) = self.cache.get(dir) else {
            return;
        };
        for entry in children {
            let path = if dir.is_empty() {
                entry.name.clone()
            } else {
                format!("{dir}/{}", entry.name)
            };
            if !self.show_set.contains(&path) {
                continue;
            }
            rows.push(VisibleRow {
                path: path.clone(),
                name: entry.name.clone(),
                is_dir: entry.is_dir,
                depth,
                expanded: entry.is_dir,
            });
            if entry.is_dir {
                self.push_children_filtered(&path, depth + 1, rows);
            }
        }
    }

    fn push_children(&self, dir: &str, depth: usize, rows: &mut Vec<VisibleRow>) {
        let Some(children) = self.cache.get(dir) else {
            return;
        };
        for entry in children {
            let path = if dir.is_empty() {
                entry.name.clone()
            } else {
                format!("{dir}/{}", entry.name)
            };
            let expanded = entry.is_dir && self.expanded.contains(&path);
            rows.push(VisibleRow {
                path: path.clone(),
                name: entry.name.clone(),
                is_dir: entry.is_dir,
                depth,
                expanded,
            });
            if expanded {
                self.push_children(&path, depth + 1, rows);
            }
        }
    }

    /// Used to persist/restore the cursor across sessions and refreshes.
    pub fn selected_path(&self) -> Option<String> {
        self.rows().get(self.selected).map(|r| r.path.clone())
    }

    /// So a collapse or refresh can never leave the cursor past the end.
    pub fn clamp_selection(&mut self, row_count: usize) {
        if row_count == 0 {
            self.selected = 0;
        } else {
            self.selected = self.selected.min(row_count - 1);
        }
    }
}

/// Parent directory of a repo-relative path; `None` for a top-level entry,
/// whose parent is the root, which has no selectable row.
pub fn parent_path(path: &str) -> Option<&str> {
    path.rfind('/').map(|i| &path[..i])
}

/// Guards a restored session: it is read from disk, so a hand-edited
/// `tree_expanded` entry containing `..`, a leading `/`, or a drive prefix
/// would otherwise let the tree read outside the working tree.
pub fn is_safe_rel_path(rel: &str) -> bool {
    use std::path::Component;
    !rel.is_empty()
        && std::path::Path::new(rel)
            .components()
            .all(|c| matches!(c, Component::Normal(_) | Component::CurDir))
}

#[cfg(test)]
mod tests;
