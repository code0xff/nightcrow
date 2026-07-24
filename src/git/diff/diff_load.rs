use crate::git::diff::snapshot::{binary_diff_hunk, path_from_delta};
use crate::git::diff::types::{ChangedFile, DiffHunk, DiffLine, LineKind, StatusKind};
use anyhow::{Context, Result};
use git2::{Diff, DiffDelta, DiffOptions, Oid, Repository};
use std::cell::RefCell;

pub const MAX_FILE_VIEW_BYTES: usize = 5 * 1024 * 1024;

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

fn decode_file_view(bytes: &[u8]) -> Result<String> {
    if bytes.len() > MAX_FILE_VIEW_BYTES {
        return Err(anyhow::anyhow!(
            "file too large to preview: {} bytes",
            bytes.len()
        ));
    }
    std::str::from_utf8(bytes)
        .map(String::from)
        .map_err(|_| anyhow::anyhow!("binary or non-utf8 file"))
}

pub fn load_workdir_file(repo: &Repository, file_path: &str) -> Result<String> {
    let workdir = repo
        .workdir()
        .ok_or_else(|| anyhow::anyhow!("bare repository"))?;
    let full = crate::git::path::resolve_in_workdir(workdir, file_path)?;
    // Size-check through the open handle rather than a second path lookup, so
    // the file that gets read is the one that was measured.
    let file = std::fs::File::open(&full).with_context(|| format!("failed to open {file_path}"))?;
    let len = file
        .metadata()
        .with_context(|| format!("failed to stat {file_path}"))?
        .len();
    // Reject a multi-GB log file or build artifact before it ever materializes
    // into memory: `decode_file_view`'s post-read length check would otherwise
    // allocate the full buffer before bailing.
    if len > MAX_FILE_VIEW_BYTES as u64 {
        return Err(anyhow::anyhow!("file too large to preview: {len} bytes"));
    }
    let mut bytes = Vec::with_capacity(len as usize);
    {
        use std::io::Read;
        // Cap the read itself: `len` came from the handle, but a file that
        // grows between the stat and the read would otherwise be read in full.
        file.take(MAX_FILE_VIEW_BYTES as u64 + 1)
            .read_to_end(&mut bytes)
            .with_context(|| format!("failed to read {file_path}"))?;
    }
    decode_file_view(&bytes)
}

pub fn load_commit_file_blob(
    repo: &Repository,
    oid: Oid,
    file_path: &str,
    status: StatusKind,
) -> Result<String> {
    let commit = repo.find_commit(oid).context("failed to find commit")?;
    let tree = if status == StatusKind::Deleted {
        commit
            .parent(0)
            .context("deleted file has no parent commit")?
            .tree()
            .context("failed to get parent tree")?
    } else {
        commit.tree().context("failed to get commit tree")?
    };
    let entry = tree
        .get_path(std::path::Path::new(file_path))
        .with_context(|| format!("path not in commit: {file_path}"))?;
    let blob = repo.find_blob(entry.id()).context("failed to read blob")?;
    decode_file_view(blob.content())
}

pub fn load_file_diff(repo: &Repository, file_path: &str) -> Result<Vec<DiffHunk>> {
    let head_tree = repo.head().ok().and_then(|head| head.peel_to_tree().ok());
    let mut diff_opts = diff_options(Some(file_path));

    let mut diff = repo
        .diff_tree_to_workdir_with_index(head_tree.as_ref(), Some(&mut diff_opts))
        .context("failed to get diff")?;

    diff.find_similar(None)
        .context("failed to detect renamed files")?;

    collect_diff_hunks(&diff, file_path)
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
    collect_commit_diff_hunks(&diff)
}

pub fn load_commit_diff(repo: &Repository, oid: Oid) -> Result<Vec<DiffHunk>> {
    let diff = commit_diff(repo, oid, None)?;
    collect_commit_diff_hunks(&diff)
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

/// Shared hunk/line accumulation logic. `on_file` returns `Some(hunk)` to prepend a
/// synthetic header entry per file (used by commit diff), or `None` to skip (status diff).
fn collect_hunks(
    diff: &Diff<'_>,
    mut on_file: impl FnMut(DiffDelta<'_>) -> Option<DiffHunk>,
    binary_fallback: &str,
) -> Result<Vec<DiffHunk>> {
    let hunks: RefCell<Vec<DiffHunk>> = RefCell::new(Vec::new());
    // Tracks the current file's path between callbacks. libgit2 invokes
    // file_cb once per delta, followed by hunk_cb/line_cb for that file —
    // hunk_cb itself isn't given the delta, so we stash the path here.
    let current_path: RefCell<Option<String>> = RefCell::new(None);

    diff.foreach(
        &mut |delta, _| {
            *current_path.borrow_mut() = path_from_delta(&delta);
            if let Some(h) = on_file(delta) {
                hunks.borrow_mut().push(h);
            }
            true
        },
        Some(&mut |delta, _| {
            let path = path_from_delta(&delta).unwrap_or_else(|| binary_fallback.to_string());
            hunks.borrow_mut().push(binary_diff_hunk(&path));
            true
        }),
        Some(&mut |_, hunk| {
            let header = std::str::from_utf8(hunk.header())
                .unwrap_or("@@")
                .trim_end_matches('\n')
                .to_string();
            hunks.borrow_mut().push(DiffHunk {
                header,
                lines: Vec::new(),
                file_path: current_path.borrow().clone(),
            });
            true
        }),
        Some(&mut |_, _, line| {
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
                h.lines.push(DiffLine { kind, content });
            }
            true
        }),
    )?;

    Ok(hunks.into_inner())
}

fn collect_diff_hunks(diff: &Diff<'_>, fallback_path: &str) -> Result<Vec<DiffHunk>> {
    collect_hunks(diff, |_| None, fallback_path)
}

fn collect_commit_diff_hunks(diff: &Diff<'_>) -> Result<Vec<DiffHunk>> {
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
    )
}