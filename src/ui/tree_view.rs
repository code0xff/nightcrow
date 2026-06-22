//! State for the read-only file-tree navigator (`ViewMode::Tree`).
//!
//! `TreeView` holds a per-directory child cache plus the set of expanded
//! directories. The visible row list is *derived* from those two on demand
//! (`visible_rows`) rather than stored, so expansion state and the flattened
//! view can never drift. All directory I/O lives in `App` (`app/tree.rs`),
//! which populates `cache` lazily; this module is pure given a populated cache,
//! which keeps the flattening logic unit-testable without a filesystem.

use crate::git::tree::TreeEntry;
use crate::ui::SearchQuery;
use std::cell::Cell;
use std::collections::{BTreeSet, HashMap, HashSet};

/// One flattened, currently-visible tree row. `path` is repo-relative (the key
/// used for previews, expansion, and selection restore); `depth` drives
/// indentation; `expanded` is only ever `true` for directories.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VisibleRow {
    pub path: String,
    pub name: String,
    pub is_dir: bool,
    pub depth: usize,
    pub expanded: bool,
}

/// One entry in the flat filename-search index: the repo-relative `path` and
/// the lowercased basename used for case-insensitive substring matching. (A
/// row's directory flag is read from the cache during rendering, so it is not
/// stored here.) Built once when search opens (see `App::build_tree_index`) and
/// discarded when it closes.
#[derive(Debug, Clone)]
pub(crate) struct TreeIndexEntry {
    pub path: String,
    pub name_lower: String,
}

#[derive(Default)]
pub struct TreeView {
    /// Index into the current `visible_rows()` list.
    pub selected: usize,
    /// Horizontal scroll offset (chars) for long rows, mirroring the file/log
    /// lists. Reset to 0 whenever the selection moves to a new row.
    pub scroll_x: usize,
    /// Repo-relative directory paths that are currently expanded. The root
    /// (`""`) is implicitly expanded and is never stored here.
    pub expanded: BTreeSet<String>,
    /// Lazily-populated children, keyed by repo-relative directory path
    /// (`""` for the root). A directory absent from this map has not been read
    /// yet; an entry with an empty vec is a directory that was read and is
    /// genuinely empty (or fully filtered).
    pub cache: HashMap<String, Vec<TreeEntry>>,
    /// Memoized longest visible-row char width, keyed by row count. Mirrors
    /// `StatusView::path_width_cache`; invalidated implicitly because the key
    /// is the row count, and any structural change that matters here also
    /// changes how many rows are visible.
    pub(crate) row_width_cache: Cell<Option<(usize, usize)>>,
    /// Whether the filename-search overlay is open. While active *and* the
    /// query is non-empty (`search_filtering`), `visible_rows` returns the
    /// filtered tree — matching entries plus the ancestor directories needed to
    /// reach them — instead of the expansion-based view.
    pub search_active: bool,
    pub search_query: SearchQuery,
    /// Flat index of every entry under the root (within `max_depth`, gitignore
    /// applied), built once when search opens. Empty while search is closed.
    pub(crate) index: Vec<TreeIndexEntry>,
    /// Repo-relative paths to display while filtering: every matching entry
    /// plus all of its ancestor directories. Recomputed on each query change.
    show_set: HashSet<String>,
    /// Count of entries matching the current query (numerator of the `(m/n)`
    /// title badge).
    pub(crate) match_count: usize,
}

impl TreeView {
    /// Reset everything except config — used when switching repositories so a
    /// previous workdir's cache/expansion never leaks into the new tree.
    pub fn reset(&mut self) {
        self.selected = 0;
        self.scroll_x = 0;
        self.expanded.clear();
        self.cache.clear();
        self.cancel_search();
        self.row_width_cache.set(None);
    }

    /// Whether the search overlay is open with a non-empty query, i.e. the
    /// filtered view is in effect. An open overlay with an empty query still
    /// shows the normal expansion-based view (so the tree does not explode
    /// before the user types).
    pub fn search_filtering(&self) -> bool {
        self.search_active && !self.search_query.is_empty()
    }

    /// Close the search overlay and drop all transient search state. Safe to
    /// call when search is already closed.
    pub fn cancel_search(&mut self) {
        self.search_active = false;
        self.search_query.clear();
        self.index.clear();
        self.show_set.clear();
        self.match_count = 0;
        self.row_width_cache.set(None);
    }

    /// Recompute `show_set`/`match_count` from `index` and the current query.
    /// Each match contributes itself and every ancestor directory so the
    /// filtered view renders an unbroken path from the root down to each hit.
    pub(crate) fn recompute_filter(&mut self) {
        // Collect matches under an immutable borrow first, then mutate the
        // show-set — `index` and `show_set` are disjoint fields but both borrow
        // `self`, so they can't be touched in the same loop.
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
                // Walk ancestors; stop as soon as one is already present, since
                // its own ancestors were added on a prior insert.
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

    /// Derive the flattened list of currently-visible rows from the cache and
    /// expansion set. Only expanded, cached directories contribute children, so
    /// this never triggers I/O and never walks an unexpanded subtree. While
    /// filtering, the row list is restricted to `show_set` instead.
    pub fn visible_rows(&self) -> Vec<VisibleRow> {
        let mut rows = Vec::new();
        if self.search_filtering() {
            self.push_children_filtered("", 0, &mut rows);
        } else {
            self.push_children("", 0, &mut rows);
        }
        rows
    }

    /// Filtered variant of `push_children`: include only entries present in
    /// `show_set` (matches and their ancestors), rendering every kept directory
    /// as expanded so the full path to each match is visible.
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

    /// Clamp `selected` to the current row count so a collapse or refresh that
    /// shrinks the list can never leave the cursor past the end.
    pub fn clamp_selection(&mut self, row_count: usize) {
        if row_count == 0 {
            self.selected = 0;
        } else {
            self.selected = self.selected.min(row_count - 1);
        }
    }
}

/// Parent directory of a repo-relative path, or `None` when the path is a
/// top-level entry (whose parent is the root, which has no selectable row).
pub fn parent_path(path: &str) -> Option<&str> {
    path.rfind('/').map(|i| &path[..i])
}

/// Whether `rel` is a safe, repo-internal relative path. Paths produced during
/// normal navigation always are (they're built from `read_dir` entry names),
/// but a restored session is read from disk — a boundary where a hand-edited
/// or corrupted `tree_expanded` entry containing `..`, a leading `/`, or a
/// drive prefix would otherwise let the tree read directories outside the
/// working tree. Used to filter restored expansion paths before any directory
/// read happens.
pub fn is_safe_rel_path(rel: &str) -> bool {
    use std::path::Component;
    !rel.is_empty()
        && std::path::Path::new(rel)
            .components()
            .all(|c| matches!(c, Component::Normal(_) | Component::CurDir))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(name: &str, is_dir: bool) -> TreeEntry {
        TreeEntry {
            name: name.to_string(),
            is_dir,
        }
    }

    /// A tree with `src/` (dir) and `README.md` (file) at the root, and
    /// `main.rs` inside `src/`. Nothing expanded yet.
    fn sample() -> TreeView {
        let mut tv = TreeView::default();
        tv.cache.insert(
            "".to_string(),
            vec![entry("src", true), entry("README.md", false)],
        );
        tv.cache
            .insert("src".to_string(), vec![entry("main.rs", false)]);
        tv
    }

    #[test]
    fn visible_rows_shows_only_top_level_when_nothing_expanded() {
        let tv = sample();
        let rows = tv.visible_rows();
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].path, "src");
        assert!(rows[0].is_dir);
        assert!(!rows[0].expanded);
        assert_eq!(rows[1].path, "README.md");
    }

    #[test]
    fn visible_rows_includes_children_of_expanded_dir() {
        let mut tv = sample();
        tv.expanded.insert("src".to_string());
        let rows = tv.visible_rows();

        assert_eq!(rows.len(), 3);
        assert_eq!(rows[0].path, "src");
        assert!(rows[0].expanded);
        assert_eq!(rows[1].path, "src/main.rs");
        assert_eq!(rows[1].depth, 1);
        assert_eq!(rows[2].path, "README.md");
    }

    #[test]
    fn visible_rows_skips_expanded_dir_without_cached_children() {
        // `expanded` references a dir whose children were never read: it should
        // simply contribute no child rows rather than panic.
        let mut tv = sample();
        tv.expanded.insert("src".to_string());
        tv.cache.remove("src");

        let rows = tv.visible_rows();
        assert_eq!(rows.len(), 2);
        // The directory row itself still renders as expanded.
        assert!(rows[0].expanded);
    }

    #[test]
    fn clamp_selection_pins_cursor_inside_row_count() {
        let mut tv = sample();
        tv.selected = 9;
        tv.clamp_selection(2);
        assert_eq!(tv.selected, 1);
        tv.clamp_selection(0);
        assert_eq!(tv.selected, 0);
    }

    #[test]
    fn selected_path_follows_visible_rows() {
        let mut tv = sample();
        tv.expanded.insert("src".to_string());
        tv.selected = 1;
        assert_eq!(tv.selected_path().as_deref(), Some("src/main.rs"));
    }

    #[test]
    fn parent_path_returns_none_for_top_level() {
        assert_eq!(parent_path("README.md"), None);
        assert_eq!(parent_path("src"), None);
        assert_eq!(parent_path("src/ui/mod.rs"), Some("src/ui"));
        assert_eq!(parent_path("src/main.rs"), Some("src"));
    }

    /// Lowercased-basename index entry from a repo-relative path.
    fn idx(path: &str) -> TreeIndexEntry {
        let name = path.rsplit('/').next().unwrap_or(path);
        TreeIndexEntry {
            path: path.to_string(),
            name_lower: name.to_lowercase(),
        }
    }

    /// A deeper tree: `src/ui/mod.rs`, `src/main.rs`, `README.md`. Cache is
    /// fully populated (as `build_tree_index` would leave it) and an index is
    /// seeded so the filter can be exercised without a filesystem.
    fn indexed_sample() -> TreeView {
        let mut tv = TreeView::default();
        tv.cache.insert(
            "".to_string(),
            vec![entry("src", true), entry("README.md", false)],
        );
        tv.cache.insert(
            "src".to_string(),
            vec![entry("ui", true), entry("main.rs", false)],
        );
        tv.cache
            .insert("src/ui".to_string(), vec![entry("mod.rs", false)]);
        tv.index = vec![
            idx("src"),
            idx("README.md"),
            idx("src/ui"),
            idx("src/main.rs"),
            idx("src/ui/mod.rs"),
        ];
        tv
    }

    #[test]
    fn recompute_filter_collects_matches_and_their_ancestors() {
        let mut tv = indexed_sample();
        tv.search_query.set("main");
        tv.recompute_filter();
        assert_eq!(tv.match_count, 1);
        // The match plus the `src` ancestor; nothing else.
        let mut shown: Vec<&str> = tv.show_set.iter().map(String::as_str).collect();
        shown.sort_unstable();
        assert_eq!(shown, vec!["src", "src/main.rs"]);
    }

    #[test]
    fn filtered_visible_rows_show_match_with_ancestor_chain() {
        let mut tv = indexed_sample();
        tv.search_active = true;
        tv.search_query.set("mod");
        tv.recompute_filter();

        let rows = tv.visible_rows();
        // The whole chain src -> src/ui -> src/ui/mod.rs, each at increasing
        // depth; README.md and src/main.rs are filtered out.
        assert_eq!(rows.len(), 3);
        assert_eq!(rows[0].path, "src");
        assert_eq!(rows[0].depth, 0);
        assert!(rows[0].expanded);
        assert_eq!(rows[1].path, "src/ui");
        assert_eq!(rows[1].depth, 1);
        assert_eq!(rows[2].path, "src/ui/mod.rs");
        assert_eq!(rows[2].depth, 2);
        assert!(!rows[2].is_dir);
    }

    #[test]
    fn filter_is_case_insensitive() {
        let mut tv = indexed_sample();
        tv.search_active = true;
        tv.search_query.set("README");
        tv.recompute_filter();
        let rows = tv.visible_rows();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].path, "README.md");
    }

    #[test]
    fn empty_query_in_search_mode_keeps_normal_view() {
        let mut tv = indexed_sample();
        tv.search_active = true;
        // No query typed yet: the tree must not explode into a full expansion.
        tv.recompute_filter();
        assert!(!tv.search_filtering());
        let rows = tv.visible_rows();
        // Normal view with nothing expanded: only the two top-level entries.
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].path, "src");
        assert_eq!(rows[1].path, "README.md");
    }

    #[test]
    fn cancel_search_clears_all_transient_state() {
        let mut tv = indexed_sample();
        tv.search_active = true;
        tv.search_query.set("mod");
        tv.recompute_filter();
        tv.cancel_search();
        assert!(!tv.search_active);
        assert!(tv.search_query.is_empty());
        assert!(tv.index.is_empty());
        assert!(tv.show_set.is_empty());
        assert_eq!(tv.match_count, 0);
    }

    #[test]
    fn is_safe_rel_path_accepts_repo_internal_and_rejects_escapes() {
        assert!(is_safe_rel_path("src"));
        assert!(is_safe_rel_path("src/ui/mod.rs"));
        // Escapes / absolute / empty are rejected.
        assert!(!is_safe_rel_path(""));
        assert!(!is_safe_rel_path(".."));
        assert!(!is_safe_rel_path("../etc"));
        assert!(!is_safe_rel_path("src/../../etc"));
        assert!(!is_safe_rel_path("/etc/passwd"));
    }
}
