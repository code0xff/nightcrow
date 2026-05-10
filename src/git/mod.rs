pub mod diff;

use std::path::{Path, PathBuf};

/// Resolve an input path to the discovered repository workdir when possible.
/// If discovery fails, return the original path so the app can still open and
/// surface the git error in its status bar.
pub fn resolve_repo_path(path: impl AsRef<Path>) -> PathBuf {
    let path = path.as_ref();
    git2::Repository::discover(path)
        .ok()
        .and_then(|repo| repo.workdir().map(Path::to_path_buf))
        .unwrap_or_else(|| path.to_path_buf())
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

        assert_eq!(resolved, PathBuf::from(repo_path));
    }

    #[test]
    fn resolve_repo_path_keeps_non_repo_path() {
        let path = PathBuf::from("/not/a/repo");

        assert_eq!(resolve_repo_path(&path), path);
    }
}
