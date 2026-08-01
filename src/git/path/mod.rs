//! Validation for repository-relative paths that reach the filesystem.
//!
//! Every path that names a file inside a worktree goes through
//! [`resolve_in_workdir`] before being opened. The web surfaces route
//! caller-supplied strings to the same loaders, so the check lives at the
//! filesystem boundary rather than at each call site.

use anyhow::{Result, anyhow};
use std::path::{Component, Path, PathBuf};

/// Directory name that holds git's own state. Reading it through a file
/// preview would expose config, hooks, and object contents.
const GIT_DIR: &str = ".git";

/// Characters NTFS and some other filesystems strip from the end of a name, so
/// that `.git.` and `.git ` open the same directory as `.git`. Git defends the
/// same way (`core.protectNTFS`).
const NAME_PADDING: [char; 2] = ['.', ' '];

/// 8.3 short name NTFS generates for `.git`. It opens the same directory, so a
/// rule that only knows the long spelling is one `dir /x` away from a bypass —
/// git blocks this exact name for the same reason (`is_ntfs_dotgit`).
const GIT_SHORT_DIR: &str = "git~1";

/// True when `name` refers to the git directory on *any* filesystem this could
/// run on: case-insensitively (macOS, Windows), ignoring the trailing dots and
/// spaces that NTFS discards, and including the 8.3 short name.
///
/// Every place that decides whether a name is git's own directory must use this
/// — a second, looser spelling of the rule is how a bypass gets in.
pub fn is_git_dir_name(name: &str) -> bool {
    let name = name.trim_end_matches(NAME_PADDING);
    name.eq_ignore_ascii_case(GIT_DIR) || name.eq_ignore_ascii_case(GIT_SHORT_DIR)
}

fn is_git_dir(part: &std::ffi::OsStr) -> bool {
    // A non-UTF-8 name cannot equal the ASCII `.git` under any of these rules.
    part.to_str().is_some_and(is_git_dir_name)
}

/// True when the filesystem would read `part` as `.` or `..` even though Rust
/// parsed it as an ordinary name.
///
/// Windows drops trailing dots and spaces from every component, so `.. ` names
/// the parent directory and `..` reaches it — but `Path::components` sees one
/// `Normal(".. ")` and the `..` arm below never runs. The escape this whole
/// module exists to stop would then be spelled with one extra space.
///
/// It costs a name like `...`, legal on Unix and unnameable on Windows anyway.
fn is_traversal_after_trimming(part: &std::ffi::OsStr) -> bool {
    part.to_str().is_some_and(|name| {
        let trimmed = name.trim_end_matches(NAME_PADDING);
        trimmed.is_empty() || trimmed == "." || trimmed == ".."
    })
}

/// Validate a path used only to address an object *inside a git commit*.
///
/// Unlike [`resolve_in_workdir`], this deliberately does not stat the path:
/// a deleted file is absent from the current worktree but is still a valid
/// member of a historical commit diff. Callers must use this only with git's
/// object database, never before opening a worktree file.
pub fn validate_commit_path(relative: &str) -> Result<()> {
    if relative.is_empty() {
        return Err(anyhow!("empty path"));
    }
    if relative.contains('\0') {
        return Err(anyhow!("path contains a NUL byte"));
    }

    for component in Path::new(relative).components() {
        match component {
            Component::Normal(part) if is_git_dir(part) => {
                return Err(anyhow!("path enters the git directory: {relative}"));
            }
            Component::Normal(part) if is_traversal_after_trimming(part) => {
                return Err(anyhow!("path is not a plain relative path: {relative}"));
            }
            Component::Normal(_) => {}
            // `..`/`.`/absolute paths must not be accepted as git pathspecs.
            _ => return Err(anyhow!("path is not a plain relative path: {relative}")),
        }
    }
    Ok(())
}

/// Resolve `relative` against `workdir`, rejecting anything that could escape
/// the worktree or read git's internals.
///
/// Rejects: absolute paths, `..` and other non-plain components, any component
/// naming the git directory (see [`is_git_dir`]), embedded NUL bytes, and
/// symlinks at *any* component — not just the final one.
///
/// The returned path is the canonicalized location and is guaranteed to sit
/// under the canonicalized `workdir`. A caller that opens it still races with
/// a concurrent rename of the worktree itself; that residual TOCTOU window is
/// accepted, since every surface reaching this function is already
/// authenticated and local.
pub fn resolve_in_workdir(workdir: &Path, relative: &str) -> Result<PathBuf> {
    validate_commit_path(relative)?;

    let candidate = Path::new(relative);
    for component in candidate.components() {
        match component {
            Component::Normal(_) => {}
            // `..` escapes the worktree; `.` and root/prefix components mean
            // the path is absolute or non-normalized. Reject rather than
            // normalize, so a rejected path never silently becomes another.
            _ => return Err(anyhow!("path is not a plain relative path: {relative}")),
        }
    }

    // Walk down one component at a time so an intermediate symlink is caught
    // before it is ever traversed. Canonicalizing the whole path instead would
    // follow those links first and only then compare the result.
    let base = workdir
        .canonicalize()
        .map_err(|err| anyhow!("worktree is unavailable: {err}"))?;
    let mut resolved = base.clone();
    for component in candidate.components() {
        resolved.push(component);
        let meta = std::fs::symlink_metadata(&resolved)
            .map_err(|err| anyhow!("failed to stat {relative}: {err}"))?;
        if meta.file_type().is_symlink() {
            return Err(anyhow!("symlinks are not followed: {relative}"));
        }
    }

    // Belt and braces: the component walk already rejected every link, so this
    // can only fail if the worktree moved mid-walk.
    if !resolved.starts_with(&base) {
        return Err(anyhow!("path escapes the worktree: {relative}"));
    }
    Ok(resolved)
}

#[cfg(test)]
mod tests;
