use crate::git::diff::snapshot::{binary_diff_hunk, path_from_delta};
use crate::git::diff::types::{ChangedFile, DiffHunk, DiffLine, LineKind, StatusKind};
use anyhow::{Context, Result};
use git2::{Diff, DiffDelta, DiffOptions, Oid, Repository};
use std::cell::RefCell;

/// Parse the new-side starting line from a unified-diff hunk header like
/// `@@ -1,3 +5,7 @@ context`. Returns `None` for synthetic headers
/// (`diff <path>`, `Binary file ...`) or anything malformed.
pub fn parse_hunk_new_start(header: &str) -> Option<usize> {
    let rest = header.strip_prefix("@@ ")?;
    let after = rest.split_once(" +")?.1;
    let token: String = after.chars().take_while(|c| c.is_ascii_digit()).collect();
    if token.is_empty() {
        return None;
    }
    token.parse().ok()
}

pub fn load_file_diff(repo: &Repository, file_path: &str) -> Result<Vec<DiffHunk>> {
    let head_tree = repo.head().ok().and_then(|head| head.peel_to_tree().ok());
    let mut diff_opts = diff_options(Some(file_path));

    let mut diff = repo
        .diff_tree_to_workdir_with_index(head_tree.as_ref(), Some(&mut diff_opts))
        .context("failed to get diff")?;

    diff.find_similar(None)
        .context("failed to detect renamed files")?;

    // A conflicted path has no stage-0 index entry, and the index-aware diff
    // answers with a delta that carries no hunks — so the file the user most
    // wants to read came back blank, indistinguishable from unchanged. Asking
    // the working tree directly skips the index and shows HEAD against what is
    // on disk, conflict markers and all, which is what the file now says.
    let conflicted = diff
        .deltas()
        .any(|delta| delta.status() == git2::Delta::Conflicted);
    if conflicted {
        let mut diff_opts = diff_options(Some(file_path));
        diff = repo
            .diff_tree_to_workdir(head_tree.as_ref(), Some(&mut diff_opts))
            .context("failed to diff a conflicted path against the working tree")?;
    }

    let mut hunks = collect_diff_hunks(&diff, file_path)?;
    // Markers are not the only way to be conflicted. A modify/delete keeps our
    // version byte for byte, and so does a binary clash, so HEAD and the
    // working tree agree and this is still empty — for a row the status list
    // shows as unmerged. Say what the conflict is rather than nothing.
    if conflicted && hunks.is_empty() {
        hunks.extend(super::conflict::summary_hunk(repo, file_path));
    }
    Ok(hunks)
}

fn commit_diff<'repo>(
    repo: &'repo Repository,
    oid: Oid,
    pathspec: Option<&str>,
) -> Result<git2::Diff<'repo>> {
    let commit = repo.find_commit(oid).context("failed to find commit")?;
    let new_tree = commit.tree().context("failed to get commit tree")?;
    // Distinguish a true root commit (no parents) from a parent-lookup
    // failure on a non-root commit — bare `.ok()` previously rendered both
    // merge commits (when parent objects were unreachable) and corrupt
    // history as if the entire tree had just been added.
    let old_tree = if commit.parent_count() == 0 {
        None
    } else {
        Some(
            commit
                .parent(0)
                .context("failed to load parent commit")?
                .tree()
                .context("failed to load parent tree")?,
        )
    };
    let mut diff_opts = diff_options(pathspec);
    let mut diff = repo
        .diff_tree_to_tree(old_tree.as_ref(), Some(&new_tree), Some(&mut diff_opts))
        .context("failed to get commit diff")?;
    diff.find_similar(None)
        .context("failed to detect renames")?;
    Ok(diff)
}

pub fn load_commit_files(repo: &Repository, oid: Oid) -> Result<Vec<ChangedFile>> {
    let diff = commit_diff(repo, oid, None)?;
    let mut files = Vec::new();
    for delta in diff.deltas() {
        let kind = match delta.status() {
            git2::Delta::Added => StatusKind::Added,
            git2::Delta::Deleted => StatusKind::Deleted,
            git2::Delta::Renamed => StatusKind::Renamed,
            git2::Delta::Typechange => StatusKind::TypeChanged,
            _ => StatusKind::Modified,
        };
        // New side is the effective path; carry the old side for renames so
        // commit drill-down also renders `old -> new`.
        let path = delta
            .new_file()
            .path()
            .or_else(|| delta.old_file().path())
            .map(|p| p.to_string_lossy().to_string())
            .unwrap_or_else(|| "unknown".to_string());
        let old_path = if kind == StatusKind::Renamed {
            delta
                .old_file()
                .path()
                .map(|p| p.to_string_lossy().to_string())
                .filter(|old| old != &path)
        } else {
            None
        };
        files.push(ChangedFile::from_commit_delta(path, old_path, kind));
    }
    Ok(files)
}

pub fn load_commit_file_diff(
    repo: &Repository,
    oid: Oid,
    file_path: &str,
) -> Result<Vec<DiffHunk>> {
    let diff = commit_diff(repo, oid, Some(file_path))?;
    collect_commit_diff_hunks(&diff, Some(file_path))
}

pub fn load_commit_diff(repo: &Repository, oid: Oid) -> Result<Vec<DiffHunk>> {
    let diff = commit_diff(repo, oid, None)?;
    collect_commit_diff_hunks(&diff, None)
}

fn diff_options(pathspec: Option<&str>) -> DiffOptions {
    let mut opts = DiffOptions::new();
    opts.include_untracked(true)
        .recurse_untracked_dirs(true)
        .show_untracked_content(true)
        .show_binary(true);
    if let Some(pathspec) = pathspec {
        opts.pathspec(pathspec).disable_pathspec_match(true);
    }
    opts
}

/// True when `delta` is about `wanted`, under either of the names it carries.
///
/// The two sides differ only in a rename git paired, which it does when the
/// pathspec reached both halves — otherwise each half arrives as its own
/// `Deleted` or `Added` delta holding that half's path on both sides. Prefix
/// matching is what reaches both: a file replaced by a directory of the same
/// name (`foo` becoming `foo/x`) pairs under the pathspec `foo`, and there the
/// old side is the only side that equals what was asked for.
fn delta_is_about(delta: &DiffDelta<'_>, wanted: &str) -> bool {
    [delta.new_file().path(), delta.old_file().path()]
        .into_iter()
        .flatten()
        .any(|path| path == std::path::Path::new(wanted))
}

/// Shared hunk/line accumulation.
///
/// `on_file` returns `Some(hunk)` to prepend a synthetic header entry per file
/// (used by commit diff), or `None` to skip (status diff).
///
/// `only` narrows the result to one file: git's pathspec still matches a
/// directory as a prefix of everything beneath it, so asking for `src` answered
/// with every changed file under `src` labelled as `src`. Turning that off is
/// not an option — it is how the pathspec addresses a file at all — so the
/// deltas it over-collects are dropped here.
fn collect_hunks(
    diff: &Diff<'_>,
    mut on_file: impl FnMut(DiffDelta<'_>) -> Option<DiffHunk>,
    binary_fallback: &str,
    only: Option<&str>,
) -> Result<Vec<DiffHunk>> {
    let hunks: RefCell<Vec<DiffHunk>> = RefCell::new(Vec::new());
    // Every callback is handed the delta it belongs to, so each one answers the
    // `only` question for itself; deciding once in `file_cb` and remembering it
    // would make the result depend on a callback order libgit2 is free to
    // change.
    let wanted = |delta: &DiffDelta<'_>| only.is_none_or(|only| delta_is_about(delta, only));

    diff.foreach(
        &mut |delta, _| {
            if !wanted(&delta) {
                return true;
            }
            if let Some(h) = on_file(delta) {
                hunks.borrow_mut().push(h);
            }
            true
        },
        Some(&mut |delta, _| {
            if !wanted(&delta) {
                return true;
            }
            let path = path_from_delta(&delta).unwrap_or_else(|| binary_fallback.to_string());
            hunks.borrow_mut().push(binary_diff_hunk(&path));
            true
        }),
        Some(&mut |delta, hunk| {
            if !wanted(&delta) {
                return true;
            }
            let header = std::str::from_utf8(hunk.header())
                .unwrap_or("@@")
                .trim_end_matches('\n')
                .to_string();
            hunks.borrow_mut().push(DiffHunk {
                header,
                lines: Vec::new(),
                file_path: path_from_delta(&delta),
            });
            true
        }),
        Some(&mut |delta, _, line| {
            if !wanted(&delta) {
                return true;
            }
            let content = std::str::from_utf8(line.content())
                .unwrap_or("")
                .trim_end_matches('\n')
                .to_string();
            let kind = match line.origin() {
                '+' => LineKind::Added,
                '-' => LineKind::Removed,
                '\\' => return true,
                _ => LineKind::Context,
            };
            if let Some(h) = hunks.borrow_mut().last_mut() {
                h.lines.push(DiffLine {
                    kind,
                    content,
                    old_lineno: line.old_lineno(),
                    new_lineno: line.new_lineno(),
                });
            }
            true
        }),
    )?;

    Ok(hunks.into_inner())
}

fn collect_diff_hunks(diff: &Diff<'_>, requested_path: &str) -> Result<Vec<DiffHunk>> {
    collect_hunks(diff, |_| None, requested_path, Some(requested_path))
}

fn collect_commit_diff_hunks(diff: &Diff<'_>, only: Option<&str>) -> Result<Vec<DiffHunk>> {
    collect_hunks(
        diff,
        |delta| {
            let path = path_from_delta(&delta).unwrap_or_else(|| "unknown".to_string());
            Some(DiffHunk {
                header: format!("diff {path}"),
                lines: Vec::new(),
                file_path: Some(path),
            })
        },
        "unknown",
        only,
    )
}
