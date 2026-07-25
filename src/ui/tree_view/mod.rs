//! State for the read-only file-tree navigator (`ViewMode::Tree`).
//! `TreeView` holds a per-directory child cache plus the set of expanded
//! directories; the visible row list is *derived* from those on demand
//! (`visible_rows`), so expansion state and the flattened view can never
//! drift. All directory I/O lives in `App` (`app/tree.rs`); this module is
//! pure given a populated cache, which keeps the flattening logic
//! unit-testable without a filesystem.

use crate::git::tree::TreeEntry;
use crate::ui::SearchQuery;
use std::cell::Cell;
use std::collections::{BTreeSet, HashMap, HashSet};

/// One flattened, currently-visible tree row. `path` is repo-relative;
/// `expanded` is only ever `true` for directories.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VisibleRow {
    pub path: String,
    pub name: String,
    pub is_dir: bool,
    pub depth: usize,
    pub expanded: bool,
}

/// One entry in the flat filename-search index. Built once when search opens
/// (see `App::build_tree_index`) and discarded when it closes.
#[derive(Debug, Clone)]
pub(crate) struct TreeIndexEntry {
    pub path: String,
    pub name_lower: String,
}

#[derive(Default)]
pub struct TreeView {
    pub selected: usize,
    /// Horizontal scroll offset (chars). Reset to 0 when the selection moves.
    pub scroll_x: usize,
    /// Repo-relative expanded directory paths. The root (`""`) is implicitly
    /// expanded and never stored here.
    pub expanded: BTreeSet<String>,
    /// Lazily-populated children, keyed by repo-relative directory path (`""`
    /// for the root). Absent = unread; empty vec = read and genuinely empty.
    pub cache: HashMap<String, Vec<TreeEntry>>,
    /// Memoized longest visible-row char width, keyed by row count. Invalidated
    /// implicitly because structural changes also change the row count.
    pub(crate) row_width_cache: Cell<Option<(usize, usize)>>,
    /// While active *and* the query is non-empty (`search_filtering`),
    /// `visible_rows` returns the filtered tree instead of the expansion view.
    pub search_active: bool,
    pub search_query: SearchQuery,
    /// Flat index of every entry under the root, built when search opens.
    pub(crate) index: Vec<TreeIndexEntry>,
    /// Repo-relative paths to display while filtering: matches plus ancestors.
    show_set: HashSet<String>,
    /// Count of entries matching the current query (`(m/n)` badge numerator).
    pub(crate) match_count: usize,
}

impl TreeView {
    /// Whether the search overlay is open with a non-empty query. An open
    /// overlay with an empty query still shows the expansion view so the tree
    /// does not explode before the user types.
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
    }

    /// Recompute `show_set`/`match_count` from `index` and the current query.
    /// Each match contributes itself and every ancestor so the filtered view
    /// renders an unbroken path from the root to each hit.
    pub(crate) fn recompute_filter(&mut self) {
        // Collect matches under an immutable borrow first, then mutate the
        // show-set — `index` and `show_set` are disjoint fields but both
        // borrow `self`, so they can't be touched in the same loop.
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
    }

    /// Derive the flattened visible rows from the cache and expansion set.
    /// Only expanded, cached directories contribute children, so this never
    /// triggers I/O. While filtering, the row list is restricted to `show_set`.
    pub fn visible_rows(&self) -> Vec<VisibleRow> {
        let mut rows = Vec::new();
        if self.search_filtering() {
            self.push_children_filtered("", 0, &mut rows);
        } else {
            self.push_children("", 0, &mut rows);
        }
        rows
    }

    /// Filtered variant of `push_children`: include only `show_set` entries,
    /// rendering every kept directory as expanded so the full path to each
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

    /// Repo-relative path of the currently selected row, if any. Used to
    /// persist/restore the cursor across sessions and refreshes.
    pub fn selected_path(&self) -> Option<String> {
        self.visible_rows()
            .get(self.selected)
            .map(|r| r.path.clone())
    }

    /// Clamp `selected` to the row count so a collapse or refresh can never
    /// leave the cursor past the end.
    pub fn clamp_selection(&mut self, row_count: usize) {
        if row_count == 0 {
            self.selected = 0;
        } else {
            self.selected = self.selected.min(row_count - 1);
        }
    }
}

/// Parent directory of a repo-relative path, or `None` for a top-level entry
/// (whose parent is the root, which has no selectable row).
pub fn parent_path(path: &str) -> Option<&str> {
    path.rfind('/').map(|i| &path[..i])
}

/// Whether `rel` is a safe, repo-internal relative path. Paths from normal
/// navigation always are, but a restored session is read from disk — a
/// hand-edited `tree_expanded` entry containing `..`, a leading `/`, or a
/// drive prefix would otherwise let the tree read outside the working tree.
pub fn is_safe_rel_path(rel: &str) -> bool {
    use std::path::Component;
    !rel.is_empty()
        && std::path::Path::new(rel)
            .components()
            .all(|c| matches!(c, Component::Normal(_) | Component::CurDir))
}

#[cfg(test)]
mod tests;
