pub mod clone;
pub mod diff;
pub mod path;
pub mod tree;

use std::path::{Path, PathBuf};

/// Resolve an input path to the discovered repository workdir when possible.
/// If discovery fails, return the original path so the app can still open and
/// surface the git error in its status bar.
pub fn resolve_repo_path(path: impl AsRef<Path>) -> PathBuf {
    let path = path.as_ref();
    git2::Repository::discover(path)
        .ok()
        .and_then(|repo| repo.workdir().map(Path::to_path_buf))
        // Not a git repo: fall back to the directory itself, canonicalized so
        // two spellings of one directory (`/w` vs `/w/`, a relative path vs
        // its absolute form) produce the same string. Project de-duplication
        // compares these paths, so differing spellings would open a second tab
        // on the same worktree. Canonicalization can only fail for a path that
        // does not exist, which the caller has already rejected.
        .unwrap_or_else(|| {
            crate::platform::paths::canonicalize_clean(path).unwrap_or_else(|_| path.to_path_buf())
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_util::make_repo;

    #[test]
    fn resolve_repo_path_uses_workdir_for_nested_path() {
        let (_dir, repo_path) = make_repo();
        let nested = Path::new(&repo_path).join("src");
        std::fs::create_dir(&nested).unwrap();

        let resolved = resolve_repo_path(&nested);

        // libgit2 returns the workdir with platform-specific symlink resolution
        // (e.g. macOS surfaces /private/var instead of /var) and a trailing
        // separator. Canonicalize both sides so the assertion checks structural
        // equality rather than literal byte equality.
        assert_eq!(
            resolved.canonicalize().unwrap(),
            PathBuf::from(repo_path).canonicalize().unwrap()
        );
    }

    #[test]
    fn resolve_repo_path_keeps_non_repo_path() {
        // A path that does not exist cannot be canonicalized, so it is
        // returned as typed rather than being dropped.
        let path = PathBuf::from("/not/a/repo");

        assert_eq!(resolve_repo_path(&path), path);
    }

    #[test]
    fn resolve_repo_path_canonicalizes_a_non_repo_directory() {
        // Project de-duplication compares these strings, so two spellings of
        // one directory must not resolve to two different paths — that would
        // open a second tab on the same worktree.
        let dir = tempfile::TempDir::new().unwrap();
        let plain = dir.path().to_path_buf();
        let with_trailing_slash = PathBuf::from(format!("{}/", plain.display()));
        let via_dot = plain.join(".");

        let resolved = resolve_repo_path(&plain);

        assert_eq!(resolve_repo_path(&with_trailing_slash), resolved);
        assert_eq!(resolve_repo_path(&via_dot), resolved);
    }
}
