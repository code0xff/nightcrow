//! Saying what a conflict is when there is nothing to diff.
//!
//! A conflicted path with markers in it diffs against HEAD like any other
//! change. The rest do not: git leaves our version on disk for a modify/delete,
//! keeps ours for a binary clash, and a rename/rename leaves a file that never
//! differed from HEAD at all. Those answer with no hunks, which on screen is
//! indistinguishable from a file nobody touched — for a row the status list is
//! showing as unmerged.

use crate::git::diff::types::{DiffHunk, DiffLine, LineKind};
use git2::Repository;

/// How `path` is conflicted, in git's own words for the same shapes
/// (`git status` calls them the same thing), or `None` if it is not.
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
            (_, true, true) => "both modified",
            (true, true, false) => "deleted by them",
            (true, false, true) => "deleted by us",
            (false, true, false) => "added by us",
            (false, false, true) => "added by them",
            _ => "both deleted",
        },
    )
}

/// One synthetic hunk naming the conflict, shaped like the one a binary change
/// gets: a header and a single line belonging to neither side, so a reader —
/// and the viewer's "is this text?" check — treats it as something to read
/// rather than something to edit against line numbers.
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
