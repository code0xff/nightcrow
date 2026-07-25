use crate::git::diff::types::{
    ChangedFile, DiffHunk, DiffLine, LineKind, RepoSnapshot, StatusKind, TrackingStatus,
};
use anyhow::{Context, Result};
use git2::{Branch, DiffDelta, Repository, Status, StatusEntry, StatusOptions};
use std::collections::BTreeMap;

fn load_tracking_status(repo: &Repository) -> Option<TrackingStatus> {
    let head = repo.head().ok()?;
    if !head.is_branch() {
        return None;
    }
    let branch = Branch::wrap(head);
    let upstream = branch.upstream().ok()?;
    let local_oid = branch.get().target()?;
    let upstream_oid = upstream.get().target()?;
    let (ahead, behind) = repo.graph_ahead_behind(local_oid, upstream_oid).ok()?;
    Some(TrackingStatus { ahead, behind })
}

pub fn load_snapshot(repo: &Repository) -> Result<RepoSnapshot> {
    let mut opts = StatusOptions::new();
    opts.include_untracked(true)
        .recurse_untracked_dirs(true)
        .renames_head_to_index(true)
        .renames_index_to_workdir(true);

    let statuses = repo
        .statuses(Some(&mut opts))
        .context("failed to get repository status")?;

    // Keyed by effective (new-side) path so the file list stays in a stable
    // sorted order across refreshes — selection restoration depends on that.
    // Each git status entry already carries both X and Y bits, so there is no
    // longer a first-wins collapse: one entry maps to one row.
    let mut files = BTreeMap::new();
    for entry in statuses.iter() {
        let Some((index, worktree)) = status_columns(entry.status()) else {
            continue;
        };
        let Some((path, old_path)) = paths_from_status_entry(&entry) else {
            continue;
        };
        if path.is_empty() {
            continue;
        }
        files.insert(
            path.clone(),
            ChangedFile::from_status_columns(path, old_path, index, worktree),
        );
    }

    let files = files.into_values().collect();

    let tracking = load_tracking_status(repo);
    let head = repo.head().ok();
    let head_oid = head.as_ref().and_then(|h| h.target());
    let branch_name = head
        .as_ref()
        .filter(|h| h.is_branch())
        .and_then(|h| h.shorthand().ok().map(String::from));
    Ok(RepoSnapshot {
        files,
        tracking,
        head_oid,
        branch_name,
    })
}

/// Map a git2 status bitset into separate index (X) and worktree (Y) columns.
/// Untracked and conflicted are reported as both-column sentinels so the
/// renderer can collapse them to `??` / `UU`. Returns `None` when neither
/// column carries a displayable change.
fn status_columns(status: Status) -> Option<(StatusKind, StatusKind)> {
    // Untracked: git renders `??` (both columns), not ` ?`. Only a *purely*
    // untracked entry collapses to `??`. A combined state such as
    // `INDEX_DELETED | WT_NEW` (staged deletion, then a fresh file recreated at
    // the same path) keeps its index status so the staged change is not hidden;
    // git itself emits two rows there, but our one-row-per-path model preserves
    // the index side (`D `) rather than masking it as untracked.
    let index_bits = Status::INDEX_NEW
        | Status::INDEX_MODIFIED
        | Status::INDEX_DELETED
        | Status::INDEX_RENAMED
        | Status::INDEX_TYPECHANGE;
    if status.contains(Status::WT_NEW) && !status.intersects(index_bits) {
        return Some((StatusKind::Untracked, StatusKind::Untracked));
    }
    // Conflicts render as `UU` in the first pass; the structured columns keep
    // room for the full unmerged matrix later.
    if status.contains(Status::CONFLICTED) {
        return Some((StatusKind::Unmerged, StatusKind::Unmerged));
    }

    let index = if status.contains(Status::INDEX_NEW) {
        StatusKind::Added
    } else if status.contains(Status::INDEX_MODIFIED) {
        StatusKind::Modified
    } else if status.contains(Status::INDEX_DELETED) {
        StatusKind::Deleted
    } else if status.contains(Status::INDEX_RENAMED) {
        StatusKind::Renamed
    } else if status.contains(Status::INDEX_TYPECHANGE) {
        StatusKind::TypeChanged
    } else {
        StatusKind::Unmodified
    };

    let worktree = if status.contains(Status::WT_MODIFIED) {
        StatusKind::Modified
    } else if status.contains(Status::WT_DELETED) {
        StatusKind::Deleted
    } else if status.contains(Status::WT_RENAMED) {
        StatusKind::Renamed
    } else if status.contains(Status::WT_TYPECHANGE) {
        StatusKind::TypeChanged
    } else if status.contains(Status::WT_UNREADABLE) {
        // No standard git short code; keep it visible as a worktree change
        // rather than dropping the row (preserves prior behavior).
        StatusKind::Modified
    } else {
        StatusKind::Unmodified
    };

    if index == StatusKind::Unmodified && worktree == StatusKind::Unmodified {
        return None;
    }
    Some((index, worktree))
}

/// Effective (new-side) path plus the old path for renames. The effective
/// path drives diff/file loading; `old_path` is display/search metadata only
/// and is omitted when it equals the effective path.
fn paths_from_status_entry(entry: &StatusEntry<'_>) -> Option<(String, Option<String>)> {
    let i2w = entry.index_to_workdir();
    let h2i = entry.head_to_index();
    let status = entry.status();

    let path = i2w
        .as_ref()
        .and_then(new_path_from_delta)
        .or_else(|| h2i.as_ref().and_then(new_path_from_delta))
        .or_else(|| entry.path().ok().map(str::to_string))?;

    let old_path = if status.intersects(Status::INDEX_RENAMED | Status::WT_RENAMED) {
        // Prefer the HEAD-side original when the index carries a rename, so a
        // double-rename (`INDEX_RENAMED | WT_RENAMED`) reports the true original
        // path rather than the intermediate staged name. Fall back to the
        // worktree side for a pure unstaged rename.
        let from = if status.contains(Status::INDEX_RENAMED) {
            h2i.as_ref()
        } else {
            i2w.as_ref()
        };
        from.and_then(old_path_from_delta)
            .filter(|old| old != &path)
    } else {
        None
    };

    Some((path, old_path))
}

fn new_path_from_delta(delta: &DiffDelta<'_>) -> Option<String> {
    delta
        .new_file()
        .path()
        .map(|p| p.to_string_lossy().to_string())
}

fn old_path_from_delta(delta: &DiffDelta<'_>) -> Option<String> {
    delta
        .old_file()
        .path()
        .map(|p| p.to_string_lossy().to_string())
}

pub(super) fn path_from_delta(delta: &DiffDelta<'_>) -> Option<String> {
    delta
        .new_file()
        .path()
        .or_else(|| delta.old_file().path())
        .map(|p| p.to_string_lossy().to_string())
}

pub(super) fn binary_diff_hunk(file_path: &str) -> DiffHunk {
    DiffHunk {
        header: format!("Binary file {file_path} changed"),
        lines: vec![DiffLine {
            kind: LineKind::Context,
            content: "Binary files differ".to_string(),
        }],
        file_path: Some(file_path.to_string()),
    }
}
