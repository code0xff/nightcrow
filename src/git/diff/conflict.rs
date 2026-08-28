//! Names for conflicts that have nothing to diff against HEAD.
//!
//! Most unmerged shapes leave no hunks — on screen that reads as a file nobody
//! touched, for a row the status list shows as unmerged — so each gets a
//! synthetic hunk saying what the conflict is.

use crate::git::diff::types::{DiffHunk, DiffLine, LineKind};
use git2::Repository;

/// How `path` is conflicted, worded the way `git status` words the same
/// shapes, or `None` if it is not conflicted.
fn describe(repo: &Repository, path: &str) -> Option<&'static str> {
    let index = repo.index().ok()?;
    let wanted = path.as_bytes();
    let entry = index
        .conflicts()
        .ok()?
        .filter_map(Result::ok)
        .find(|conflict| {
            [&conflict.ancestor, &conflict.our, &conflict.their]
                .into_iter()
                .flatten()
                .any(|stage| stage.path == wanted)
        })?;
    Some(
        match (
            entry.ancestor.is_some(),
            entry.our.is_some(),
            entry.their.is_some(),
        ) {
            (true, true, true) => "both modified",
            // No ancestor means neither side is modifying anything — the path
            // is new to both. Folding this into "both modified" told the reader
            // a common version exists for a file that never had one.
            (false, true, true) => "both added",
            (true, true, false) => "deleted by them",
            (true, false, true) => "deleted by us",
            (false, true, false) => "added by us",
            (false, false, true) => "added by them",
            _ => "both deleted",
        },
    )
}

/// One synthetic hunk naming the conflict, shaped like a binary change's: a
/// header plus a line belonging to neither side, so a reader — and the
/// viewer's "is this text?" check — reads it as text, not line-numbered edits.
pub(super) fn summary_hunk(repo: &Repository, path: &str) -> Option<DiffHunk> {
    let description = describe(repo, path)?;
    Some(DiffHunk {
        header: format!("Unmerged path {path}"),
        lines: vec![DiffLine {
            kind: LineKind::Context,
            content: format!("{description} — nothing differs from HEAD to show"),
            old_lineno: None,
            new_lineno: None,
        }],
        file_path: Some(path.to_string()),
    })
}
