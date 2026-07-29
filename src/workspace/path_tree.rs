//! Directory browser for the repo dialog's path field.
//!
//! A flat row list, not a nested tree: expanding splices a directory's children
//! in after it and collapsing removes the rows below it, so the selection is a
//! plain index into what is on screen and no flatten pass runs per frame.
//!
//! Directories only, and nothing here writes: the browser fills the field, and
//! the field's own Enter stays the single place a repo is actually opened. It
//! deliberately does not reuse `git::tree`, which requires a `git2::Repository`
//! and refuses paths outside a worktree — the browser has to walk directories
//! belonging to no repo, with possibly no project open at all.

use super::path_complete::{is_sep, read_dir_names, split_dir};
use crate::platform::paths::expand_tilde;
use std::path::{MAIN_SEPARATOR, Path, PathBuf};

/// One visible row: a directory name `depth` levels below the root.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PathRow {
    pub name: String,
    pub depth: usize,
    /// Whether this row's children are spliced in below it. Set even when the
    /// directory turned out to have none, so the marker shows it was read
    /// rather than leaving the user pressing `→` at an unchanging row.
    pub expanded: bool,
}

#[derive(Debug, Clone)]
pub struct PathTree {
    /// The root exactly as the user typed it (`~/coding`, `..`, or `""` for the
    /// cwd). Kept beside `root` so a picked path is assembled from the user's
    /// own notation — the dialog never rewrites their `~` into an absolute path.
    root_text: String,
    /// The canonicalized root. Canonical because stepping the root up walks
    /// `parent()`, which yields nothing useful for a relative path like `.`.
    root: PathBuf,
    /// Separator to assemble picked paths with: whatever the field already uses,
    /// so a path typed with `/` on Windows doesn't come back with a `\` in it.
    sep: char,
    rows: Vec<PathRow>,
    selected: usize,
}

impl PathTree {
    /// Open the browser on the directory the field currently names. `None` when
    /// that cannot be read, which the caller reports on the notice row — with no
    /// rows and no root there is nothing to draw.
    pub(crate) fn open(buf: &str) -> Option<Self> {
        let trimmed = buf.trim();
        let sep = trimmed
            .chars()
            .rev()
            .find(|c| is_sep(*c))
            .unwrap_or(MAIN_SEPARATOR);
        // Browse from the directory the field names; when it names a file or a
        // half-typed component, fall back to the text up to the last separator —
        // the same reading Tab completion gives it.
        let text = if !trimmed.is_empty() && expand_tilde(trimmed).is_dir() {
            trimmed
        } else {
            split_dir(trimmed).0
        };
        let root =
            std::fs::canonicalize(expand_tilde(if text.is_empty() { "." } else { text })).ok()?;
        if !root.is_dir() {
            return None;
        }
        let rows = list_rows(&root, 0);
        Some(Self {
            root_text: text.to_string(),
            root,
            sep,
            rows,
            selected: 0,
        })
    }

    /// The root as the user's own text, for the browser's title. `""` means the
    /// cwd, which reads as `.` on screen.
    pub fn root_label(&self) -> &str {
        if self.root_text.is_empty() {
            "."
        } else {
            &self.root_text
        }
    }

    pub fn rows(&self) -> &[PathRow] {
        &self.rows
    }

    pub fn selected(&self) -> usize {
        self.selected
    }

    /// Clamped rather than wrapping: the list is a path being narrowed down, and
    /// wrapping from the last row back to the first loses the user's place.
    pub(crate) fn move_selection(&mut self, down: bool) {
        if self.rows.is_empty() {
            return;
        }
        self.selected = if down {
            (self.selected + 1).min(self.rows.len() - 1)
        } else {
            self.selected.saturating_sub(1)
        };
    }

    /// Read the selected directory's children and splice them in below it. One
    /// `read_dir` per press, against that directory only — an unexpanded subtree
    /// is never walked.
    pub(crate) fn expand(&mut self) {
        let Some(row) = self.rows.get(self.selected) else {
            return;
        };
        if row.expanded {
            return;
        }
        let depth = row.depth;
        let children = list_rows(&self.abs_of(self.selected), depth + 1);
        self.rows[self.selected].expanded = true;
        let at = self.selected + 1;
        self.rows.splice(at..at, children);
    }

    /// Collapse the selected directory, or — when it is already collapsed — step
    /// out of it: to its parent row, or past the root itself at depth 0.
    pub(crate) fn collapse_or_up(&mut self) {
        let Some(row) = self.rows.get(self.selected) else {
            // Nothing to collapse in an empty directory, so `←` still means
            // "get me out of here".
            self.re_root();
            return;
        };
        let depth = row.depth;
        if row.expanded {
            self.rows[self.selected].expanded = false;
            let from = self.selected + 1;
            let end = self.rows[from..]
                .iter()
                .position(|r| r.depth <= depth)
                .map_or(self.rows.len(), |n| from + n);
            self.rows.drain(from..end);
            return;
        }
        if depth > 0 {
            if let Some(i) = self.rows[..self.selected]
                .iter()
                .rposition(|r| r.depth == depth - 1)
            {
                self.selected = i;
            }
            return;
        }
        self.re_root();
    }

    /// The picked path in the user's own notation, with a trailing separator so
    /// Tab can carry on descending from it once the field has it back.
    pub(crate) fn selected_path(&self) -> String {
        let mut out = self.root_text.clone();
        if !self.rows.is_empty() {
            for name in self.components_of(self.selected) {
                if !out.is_empty() && !out.ends_with(is_sep) {
                    out.push(self.sep);
                }
                out.push_str(name);
            }
        }
        // An empty root text is the cwd; a bare separator would read as the
        // filesystem root instead.
        if out.is_empty() {
            out.push('.');
        }
        if !out.ends_with(is_sep) {
            out.push(self.sep);
        }
        out
    }

    /// Step the root up one level, so a browse that started deep in one checkout
    /// can still reach a sibling. Expansion below is dropped — the rows are
    /// rebuilt from the new root — and the directory just left is selected, so
    /// the key reads as "step out" rather than "jump somewhere".
    fn re_root(&mut self) {
        let Some(parent) = self.root.parent().map(Path::to_path_buf) else {
            return;
        };
        let left = self
            .root
            .file_name()
            .and_then(|n| n.to_str())
            .map(str::to_string);
        // Verify the user-notation parent against the real one instead of
        // trusting the text surgery: `~` has no expressible parent, and neither
        // does a bare Windows drive. Falling back to the absolute path is the
        // one place the dialog rewrites the user's text, because their notation
        // cannot name where they just asked to go.
        self.root_text = parent_text(&self.root_text, self.sep)
            .filter(|t| canonicalizes_to(t, &parent))
            .unwrap_or_else(|| parent.to_string_lossy().to_string());
        self.root = parent;
        self.rows = list_rows(&self.root, 0);
        self.selected = left
            .and_then(|name| self.rows.iter().position(|r| r.name == name))
            .unwrap_or(0);
    }

    /// The selected row's path components, walking back up the flat list: the
    /// nearest preceding row one level shallower is its parent.
    fn components_of(&self, idx: usize) -> Vec<&str> {
        let mut want = self.rows[idx].depth;
        let mut out = Vec::with_capacity(want + 1);
        for row in self.rows[..=idx].iter().rev() {
            if row.depth == want {
                out.push(row.name.as_str());
                if want == 0 {
                    break;
                }
                want -= 1;
            }
        }
        out.reverse();
        out
    }

    fn abs_of(&self, idx: usize) -> PathBuf {
        self.components_of(idx)
            .iter()
            .fold(self.root.clone(), |p, c| p.join(c))
    }
}

/// Hidden directories are left out, matching the completer's default: a home
/// directory full of dot-directories would bury the checkouts being looked for.
fn list_rows(dir: &Path, depth: usize) -> Vec<PathRow> {
    read_dir_names(dir, false)
        .into_iter()
        .map(|name| PathRow {
            name,
            depth,
            expanded: false,
        })
        .collect()
}

/// The user's root text one level up, or `None` when their notation cannot
/// express it. Purely textual — the caller checks it against the real parent.
fn parent_text(text: &str, sep: char) -> Option<String> {
    let t = text.trim_end_matches(is_sep);
    if t.is_empty() {
        // `""` is the cwd, whose parent is `..`. All-separators is the
        // filesystem root, which has no parent for `re_root` to reach.
        return text.is_empty().then(|| "..".to_string());
    }
    if t == "." {
        return Some("..".to_string());
    }
    // `..` can only go further up by appending another one; trimming a
    // component off it would walk back down.
    if split_dir(t).1 == ".." {
        return Some(format!("{t}{sep}.."));
    }
    match t.char_indices().rfind(|(_, c)| is_sep(*c)) {
        // A separator at index 0 is the filesystem root itself, which stays.
        Some((0, c)) => Some(c.to_string()),
        Some((i, _)) => Some(t[..i].to_string()),
        // A lone relative component (`nightcrow`) sits in the cwd.
        None => Some(".".to_string()),
    }
}

fn canonicalizes_to(text: &str, expected: &Path) -> bool {
    std::fs::canonicalize(expand_tilde(text)).is_ok_and(|p| p == expected)
}

#[cfg(test)]
mod tests;
