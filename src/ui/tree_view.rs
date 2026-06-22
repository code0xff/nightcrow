//! State for the read-only file-tree navigator (`ViewMode::Tree`).
//!
//! `TreeView` holds a per-directory child cache plus the set of expanded
//! directories. The visible row list is *derived* from those two on demand
//! (`visible_rows`) rather than stored, so expansion state and the flattened
//! view can never drift. All directory I/O lives in `App` (`app/tree.rs`),
//! which populates `cache` lazily; this module is pure given a populated cache,
//! which keeps the flattening logic unit-testable without a filesystem.

use crate::git::tree::TreeEntry;
use std::cell::Cell;
use std::collections::{BTreeSet, HashMap};

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
}

impl TreeView {
    /// Reset everything except config — used when switching repositories so a
    /// previous workdir's cache/expansion never leaks into the new tree.
    pub fn reset(&mut self) {
        self.selected = 0;
        self.scroll_x = 0;
        self.expanded.clear();
        self.cache.clear();
        self.row_width_cache.set(None);
    }

    /// Whether `dir` (repo-relative) is currently expanded. The root is always
    /// considered expanded so its children form the top level.
    pub fn is_expanded(&self, dir: &str) -> bool {
        dir.is_empty() || self.expanded.contains(dir)
    }

    /// Derive the flattened list of currently-visible rows from the cache and
    /// expansion set. Only expanded, cached directories contribute children, so
    /// this never triggers I/O and never walks an unexpanded subtree.
    pub fn visible_rows(&self) -> Vec<VisibleRow> {
        let mut rows = Vec::new();
        self.push_children("", 0, &mut rows);
        rows
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
    fn is_expanded_treats_root_as_always_open() {
        let tv = TreeView::default();
        assert!(tv.is_expanded(""));
        assert!(!tv.is_expanded("src"));
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
}
