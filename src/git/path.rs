//! Validation for repository-relative paths that reach the filesystem.
//!
//! Every path that names a file *inside* a worktree goes through
//! [`resolve_in_workdir`] before being opened. Today's callers pass paths that
//! git itself produced, but the web surfaces route caller-supplied strings to
//! the same loaders, so the check lives at the filesystem boundary rather than
//! at each call site.

use anyhow::{Result, anyhow};
use std::path::{Component, Path, PathBuf};

/// Directory name that holds git's own state. Reading it through a file
/// preview would expose config, hooks, and object contents.
const GIT_DIR: &str = ".git";

/// Characters NTFS and some other filesystems strip from the end of a name, so
/// that `.git.` and `.git ` open the same directory as `.git`. Git defends the
/// same way (`core.protectNTFS`).
const NAME_PADDING: [char; 2] = ['.', ' '];

/// True when `name` refers to the git directory on *any* filesystem this could
/// run on: case-insensitively (macOS, Windows) and ignoring the trailing dots
/// and spaces that NTFS discards.
///
/// Every place that decides whether a name is git's own directory must use this
/// — a second, looser spelling of the rule is how a bypass gets in.
pub fn is_git_dir_name(name: &str) -> bool {
    name.trim_end_matches(NAME_PADDING)
        .eq_ignore_ascii_case(GIT_DIR)
}

fn is_git_dir(part: &std::ffi::OsStr) -> bool {
    // A non-UTF-8 name cannot equal the ASCII `.git` under any of these rules.
    part.to_str().is_some_and(is_git_dir_name)
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
    if relative.is_empty() {
        return Err(anyhow!("empty path"));
    }
    if relative.contains('\0') {
        return Err(anyhow!("path contains a NUL byte"));
    }

    let candidate = Path::new(relative);
    for component in candidate.components() {
        match component {
            Component::Normal(part) => {
                if is_git_dir(part) {
                    return Err(anyhow!("path enters the git directory: {relative}"));
                }
            }
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
mod tests {
    use super::*;

    fn workdir() -> (tempfile::TempDir, PathBuf) {
        let dir = tempfile::TempDir::new().unwrap();
        let path = dir.path().canonicalize().unwrap();
        (dir, path)
    }

    #[test]
    fn resolve_in_workdir_accepts_a_nested_file() {
        let (_dir, root) = workdir();
        std::fs::create_dir(root.join("src")).unwrap();
        std::fs::write(root.join("src/main.rs"), "fn main() {}").unwrap();

        let resolved = resolve_in_workdir(&root, "src/main.rs").unwrap();

        assert_eq!(resolved, root.join("src/main.rs"));
    }

    #[test]
    fn resolve_in_workdir_rejects_parent_traversal() {
        let (_dir, root) = workdir();

        let err = resolve_in_workdir(&root, "../secrets.txt").unwrap_err();

        assert!(
            err.to_string().contains("plain relative path"),
            "unexpected error: {err}"
        );
    }

    #[test]
    fn resolve_in_workdir_rejects_traversal_hidden_mid_path() {
        let (_dir, root) = workdir();
        std::fs::create_dir(root.join("src")).unwrap();

        let err = resolve_in_workdir(&root, "src/../../secrets.txt").unwrap_err();

        assert!(
            err.to_string().contains("plain relative path"),
            "unexpected error: {err}"
        );
    }

    #[test]
    fn resolve_in_workdir_rejects_absolute_paths() {
        let (_dir, root) = workdir();

        let err = resolve_in_workdir(&root, "/etc/passwd").unwrap_err();

        assert!(
            err.to_string().contains("plain relative path"),
            "unexpected error: {err}"
        );
    }

    #[test]
    fn resolve_in_workdir_rejects_the_git_directory() {
        let (_dir, root) = workdir();
        std::fs::create_dir(root.join(GIT_DIR)).unwrap();
        std::fs::write(root.join(".git/config"), "[core]").unwrap();

        let err = resolve_in_workdir(&root, ".git/config").unwrap_err();

        assert!(
            err.to_string().contains("git directory"),
            "unexpected error: {err}"
        );
    }

    #[test]
    fn resolve_in_workdir_rejects_the_git_directory_in_any_case() {
        let (_dir, root) = workdir();

        let err = resolve_in_workdir(&root, ".GIT/config").unwrap_err();

        assert!(
            err.to_string().contains("git directory"),
            "unexpected error: {err}"
        );
    }

    #[test]
    fn resolve_in_workdir_rejects_the_git_directory_with_trailing_padding() {
        // NTFS strips trailing dots and spaces, so these name `.git` there. The
        // check must not depend on the host filesystem refusing to open them.
        let (_dir, root) = workdir();

        for padded in [".git.", ".git ", ".GIT..  ", ".git. /config"] {
            let err = resolve_in_workdir(&root, padded).unwrap_err();
            assert!(
                err.to_string().contains("git directory"),
                "{padded:?} must be rejected as the git directory, got: {err}"
            );
        }
    }

    #[test]
    fn resolve_in_workdir_allows_dotfiles_that_merely_start_with_git() {
        // `.gitignore` and `.github/` are ordinary tracked files; rejecting a
        // whole `.git*` prefix would make them unviewable.
        let (_dir, root) = workdir();
        std::fs::write(root.join(".gitignore"), "/target\n").unwrap();
        std::fs::create_dir(root.join(".github")).unwrap();
        std::fs::write(root.join(".github/ci.yml"), "on: push\n").unwrap();

        assert!(resolve_in_workdir(&root, ".gitignore").is_ok());
        assert!(resolve_in_workdir(&root, ".github/ci.yml").is_ok());
    }

    #[test]
    fn resolve_in_workdir_rejects_an_empty_path() {
        let (_dir, root) = workdir();

        assert!(resolve_in_workdir(&root, "").is_err());
    }

    #[test]
    fn resolve_in_workdir_rejects_a_nul_byte() {
        let (_dir, root) = workdir();

        let err = resolve_in_workdir(&root, "src/main.rs\0.png").unwrap_err();

        assert!(err.to_string().contains("NUL"), "unexpected error: {err}");
    }

    #[test]
    fn resolve_in_workdir_rejects_a_symlinked_leaf() {
        let (_dir, root) = workdir();
        std::fs::write(root.join("real.txt"), "data").unwrap();
        std::os::unix::fs::symlink(root.join("real.txt"), root.join("link.txt")).unwrap();

        let err = resolve_in_workdir(&root, "link.txt").unwrap_err();

        assert!(
            err.to_string().contains("symlink"),
            "unexpected error: {err}"
        );
    }

    #[test]
    fn resolve_in_workdir_rejects_a_symlinked_parent_directory() {
        // The leaf is an ordinary file; only the directory above it is a link.
        // A leaf-only check misses this and happily reads outside the worktree.
        let outside = tempfile::TempDir::new().unwrap();
        std::fs::write(outside.path().join("secrets.txt"), "token").unwrap();
        let (_dir, root) = workdir();
        std::os::unix::fs::symlink(outside.path(), root.join("escape")).unwrap();

        let err = resolve_in_workdir(&root, "escape/secrets.txt").unwrap_err();

        assert!(
            err.to_string().contains("symlink"),
            "unexpected error: {err}"
        );
    }

    #[test]
    fn resolve_in_workdir_reports_a_missing_file() {
        let (_dir, root) = workdir();

        let err = resolve_in_workdir(&root, "nope.txt").unwrap_err();

        assert!(
            err.to_string().contains("failed to stat"),
            "unexpected error: {err}"
        );
    }
}
