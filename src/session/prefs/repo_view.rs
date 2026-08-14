//! What each project was last showing in the browser, so opening it again
//! opens what was open.
//!
//! The TUI has kept this per repository since it had a session file — mode, the
//! selected file, the tree's cursor and its expanded directories
//! (`app::session_io`). This is the same thing for the viewer, and deliberately
//! not the same *file*: `workspace.json` belongs to the TUI, which rewrites it
//! whole when it exits (`session::operations::persist_workspace` says so), so an
//! entry written here would go the next time a TUI ran. Kept in `viewer.json`
//! beside `maximized`, which is per-project for the same reason and keyed the
//! same way — by absolute path, because repo ids only live as long as the
//! process.

use serde::{Deserialize, Serialize};

/// How many projects' views to remember. Past this the oldest go. Matches the
/// TUI's `MAX_REMEMBERED` and `maximized`'s cap for the same reason: a file
/// that grows with every project ever glanced at.
pub const MAX_REMEMBERED_VIEWS: usize = 50;

/// How many expanded directories one project may keep. A tree opened all the
/// way down is a long list, and restoring it is worth less the longer it gets —
/// what someone wants back is the branch they were working in.
pub const MAX_TREE_EXPANDED: usize = 200;

/// A commit id is the one value here that is not a path. Bounded and checked so
/// what comes off the file (or off a client) cannot be an arbitrary string
/// handed to git. Forty is what git2 will parse — a longer hex string is stored
/// only to fail every restore it is read into.
const MAX_OID_LEN: usize = 40;

/// Which list the sidebar was showing.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum ViewTab {
    Status,
    Log,
    Tree,
}

impl ViewTab {
    /// Parse what a client sent. Unknown strings are `None` — the wire form is
    /// a boundary input.
    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "status" => Some(Self::Status),
            "log" => Some(Self::Log),
            "tree" => Some(Self::Tree),
            _ => None,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Status => "status",
            Self::Log => "log",
            Self::Tree => "tree",
        }
    }
}

/// Which face of the file was on screen. A file has two — its diff and its
/// whole contents — and which one was showing is part of what was showing.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum ViewFace {
    Diff,
    Source,
}

impl ViewFace {
    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "diff" => Some(Self::Diff),
            "source" => Some(Self::Source),
            _ => None,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Diff => "diff",
            Self::Source => "source",
        }
    }
}

/// The file that was open, if one was.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ViewFile {
    /// Repository-relative path.
    pub path: String,
    /// The commit it was read from, when it was not the working tree's.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub commit: Option<String>,
    pub face: ViewFace,
}

/// One project's last view.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RepoView {
    /// Absolute worktree path.
    pub repo: String,
    pub tab: ViewTab,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub file: Option<ViewFile>,
    /// Repository-relative directories the tree had open. Which row was on it
    /// is not stored: the browser's tree has no cursor of its own, and the row
    /// worth coming back to is the one whose file is open — which `file` names.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tree_expanded: Vec<String>,
}

/// Record `view`, most recently set first.
///
/// Sanitised on the way in rather than trusted, because both of its sources are
/// boundaries: a hand-edited file, and a client. The HTTP layer rejects what it
/// can name (an unknown repo, an unknown tab); what is dropped here is the part
/// where "invalid" means "not a path this project can hold", and dropping it
/// leaves a view that still opens rather than a request that fails whole.
pub fn remember(list: &mut Vec<RepoView>, mut view: RepoView) {
    sanitize(&mut view);
    list.retain(|entry| entry.repo != view.repo);
    list.insert(0, view);
    list.truncate(MAX_REMEMBERED_VIEWS);
}

/// What `repo` was last showing, if anything.
pub fn view_of<'a>(list: &'a [RepoView], repo: &str) -> Option<&'a RepoView> {
    list.iter().find(|entry| entry.repo == repo)
}

/// Hold a list that came off disk to what a write would have produced.
///
/// The file is not necessarily one this build wrote — it can be hand-edited, or
/// left by a version with a different cap — and everything below is read back
/// into paths the viewer then asks the server to open.
pub fn normalize(list: &mut Vec<RepoView>) {
    let mut seen = std::collections::HashSet::new();
    // First wins, so a duplicate resolves to the entry `view_of` would have
    // found: normalizing must not change what the file means.
    list.retain(|entry| seen.insert(entry.repo.clone()));
    list.truncate(MAX_REMEMBERED_VIEWS);
    for view in list.iter_mut() {
        sanitize(view);
    }
}

/// Drop what this view cannot legitimately carry: a path reaching outside the
/// project (`..`, a leading `/`, a drive prefix), a commit id that is not one,
/// and expansion past the cap.
fn sanitize(view: &mut RepoView) {
    if let Some(file) = &view.file
        && !(is_safe(&file.path) && file.commit.as_deref().is_none_or(is_oid))
    {
        view.file = None;
    }
    view.tree_expanded.retain(|path| is_safe(path));
    view.tree_expanded.truncate(MAX_TREE_EXPANDED);
}

/// Whether `path` stays inside the project it belongs to, and is a path at all.
///
/// The first question is the TUI's, asked of its own restored session
/// (`ui::tree_view::is_safe_rel_path`). The second is this file's own: a stored
/// path is read back into a request, and a NUL is not something any of them can
/// open — keeping one only produces a failed restore later.
pub fn is_safe(path: &str) -> bool {
    !path.contains('\0') && crate::ui::tree_view::is_safe_rel_path(path)
}

/// Whether `oid` could name a commit: hex, and no longer than one.
pub fn is_oid(oid: &str) -> bool {
    !oid.is_empty() && oid.len() <= MAX_OID_LEN && oid.bytes().all(|b| b.is_ascii_hexdigit())
}

#[cfg(test)]
#[path = "repo_view_tests.rs"]
mod tests;
