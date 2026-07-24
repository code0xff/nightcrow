//! Lazy, read-only directory listing for the file-tree navigator.
//!
//! Each call reads exactly one directory level (`std::fs::read_dir`); the
//! caller decides when to descend, so an unexpanded subtree is never walked.
//! Listing is filtered against `.gitignore` (via libgit2) and repository
//! metadata, and symlinks are reported as non-directories so the navigator
//! never follows one — this is what keeps the tree free of cycles without a
//! visited-set.

use anyhow::{Context, Result};
use git2::Repository;
use std::path::Path;

/// One immediate child of a directory. `is_dir` is taken from the entry's own
/// file type (symlinks resolve to `false`), so a symlinked directory shows up
/// as a non-expandable row and is never descended into.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TreeEntry {
    pub name: String,
    pub is_dir: bool,
}

/// Read the immediate children of `rel_dir` (a repo-relative path; `""` is the
/// workdir root). Entries are filtered and returned sorted with directories
/// first, then case-sensitive alphabetical by name.
///
/// Filtering rules:
/// - `.git` is skipped at every level (repository metadata / object storage).
/// - When `respect_gitignore` is set, ignored paths are dropped via
///   `Repository::is_path_ignored`.
/// - Non-UTF-8 names are skipped: the file-view loader keys on `&str` paths and
///   cannot losslessly address them.
/// - Individual entries whose metadata cannot be read are skipped rather than
///   failing the whole listing.
pub fn read_children(
    repo: &Repository,
    workdir: &Path,
    rel_dir: &str,
    respect_gitignore: bool,
) -> Result<Vec<TreeEntry>> {
    // Same gate as the file loader: plain relative components only, no `.git`,
    // no symlink at any depth, and containment in the worktree. A stale session
    // or a cached entry swapped for a symlink on disk both arrive here, and the
    // web surfaces will route request strings straight into `rel_dir`.
    let abs_dir = if rel_dir.is_empty() {
        std::fs::canonicalize(workdir)
            .with_context(|| format!("failed to resolve workdir {}", workdir.display()))?
    } else {
        crate::git::path::resolve_in_workdir(workdir, rel_dir)?
    };
    let meta =
        std::fs::symlink_metadata(&abs_dir).with_context(|| format!("failed to stat {rel_dir}"))?;
    if !meta.file_type().is_dir() {
        anyhow::bail!("not a directory: {rel_dir}");
    }
    // Read the resolved path rather than re-joining `rel_dir`, so the directory
    // that gets listed is the one that was validated.
    let read = std::fs::read_dir(&abs_dir)
        .with_context(|| format!("failed to read directory {rel_dir}"))?;

    let mut out = Vec::new();
    for entry in read {
        let Ok(entry) = entry else { continue };
        // Non-UTF-8 names cannot be addressed by the `&str`-keyed file-view
        // loader, so they are dropped from the tree entirely.
        let Ok(name) = entry.file_name().into_string() else {
            continue;
        };
        // `.git` is repository metadata, not browsable project content. git
        // does not list it in `.gitignore`, so it must be skipped explicitly.
        // Shares the rule with the path validator: an exact `== ".git"` here
        // would still list `.GIT` on a case-insensitive filesystem.
        if crate::git::path::is_git_dir_name(&name) {
            continue;
        }
        // `file_type()` does NOT follow symlinks, so a symlinked directory
        // reports `is_dir() == false` and becomes a non-expandable row — the
        // navigator therefore never descends a link and cannot cycle.
        let Ok(file_type) = entry.file_type() else {
            continue;
        };
        let is_dir = file_type.is_dir();

        let rel_path = if rel_dir.is_empty() {
            name.clone()
        } else {
            format!("{rel_dir}/{name}")
        };
        if respect_gitignore && repo.is_path_ignored(Path::new(&rel_path)).unwrap_or(false) {
            continue;
        }
        out.push(TreeEntry { name, is_dir });
    }

    // Directories first (true sorts after false, so compare reversed), then
    // case-sensitive alphabetical — stable, predictable ordering for keyboard
    // navigation.
    out.sort_by(|a, b| b.is_dir.cmp(&a.is_dir).then_with(|| a.name.cmp(&b.name)));
    Ok(out)
}

/// One hit from [`search_tree`]: the full repo-relative path and whether the
/// entry is a directory.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TreeMatch {
    pub path: String,
    pub is_dir: bool,
}

/// Recursively search the worktree for entries whose basename contains `query`
/// (case-insensitive substring), mirroring the TUI's tree search
/// (`App::build_tree_index` + `recompute_filter`): a full walk from the root,
/// one [`read_children`] call per directory, depth-capped at `max_depth` and
/// gitignore-filtered. Symlinked directories report `is_dir == false` and so are
/// never descended (the same cycle guard that browsing relies on).
///
/// Results are sorted by path for a stable listing and capped at `limit`.
/// `max_visits` bounds how many entries the walk may inspect: unlike the TUI's
/// single-user in-process index, this runs per web request, so the traversal —
/// not just the retained results — must be bounded against a pathological or
/// hostile tree. `truncated` is true when either budget cut the walk short or
/// more matches existed than were returned. An empty `query` matches every
/// entry, so callers gate on non-empty input.
pub fn search_tree(
    repo: &Repository,
    workdir: &Path,
    query: &str,
    max_depth: usize,
    max_visits: usize,
    limit: usize,
) -> Result<(Vec<TreeMatch>, bool)> {
    let q = query.to_lowercase();
    let mut matches = Vec::new();
    let mut visited = 0usize;
    // Set when a budget (visit count or result limit) cuts the walk short, so the
    // caller can flag the listing as incomplete.
    let mut budget_hit = false;
    // (dir, depth-of-its-children): the root's children sit at depth 0, matching
    // `App::build_tree_index`'s depth accounting.
    let mut stack = vec![(String::new(), 0usize)];
    'walk: while let Some((dir, depth)) = stack.pop() {
        let children = match read_children(repo, workdir, &dir, true) {
            Ok(children) => children,
            // The root must be readable; a subdirectory that vanished mid-walk
            // (or is otherwise unreadable) is skipped rather than failing the
            // whole search.
            Err(err) if dir.is_empty() => return Err(err),
            Err(_) => continue,
        };
        for entry in children {
            // Every inspected entry counts against the visit budget, including
            // non-matching ones — that is what bounds the filesystem work.
            if visited >= max_visits {
                budget_hit = true;
                break 'walk;
            }
            visited += 1;
            let path = if dir.is_empty() {
                entry.name.clone()
            } else {
                format!("{dir}/{}", entry.name)
            };
            if entry.name.to_lowercase().contains(&q) {
                matches.push(TreeMatch {
                    path: path.clone(),
                    is_dir: entry.is_dir,
                });
            }
            // Descend only while the next level stays within max_depth, mirroring
            // the expand guard in the TUI.
            if entry.is_dir && depth < max_depth {
                stack.push((path, depth + 1));
            }
        }
    }
    matches.sort_by(|a, b| a.path.cmp(&b.path));
    let truncated = budget_hit || matches.len() > limit;
    matches.truncate(limit);
    Ok((matches, truncated))
}

#[cfg(test)]
mod tests;

