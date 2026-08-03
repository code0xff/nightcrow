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
    // The worktree git says this path belongs to, or the path itself when there
    // is no repository there.
    let found = git2::Repository::discover(path)
        .ok()
        .and_then(|repo| repo.workdir().map(Path::to_path_buf));
    let candidate = found.as_deref().unwrap_or(path);
    // Canonicalized whichever branch produced it, so one worktree has exactly
    // one spelling. Project de-duplication compares these strings, and a
    // second spelling opens a second tab on a repository already open.
    //
    // Applied to libgit2's answer too, rather than trusting it: what `workdir`
    // returns is platform-specific — a trailing separator, symlinks resolved on
    // some systems and not others, and on Windows the casing as it was asked
    // for rather than as it is on disk, where `C:\Code` and `c:\code` are one
    // directory. Making the guarantee ours costs one `stat` and does not depend
    // on behaviour no test here can reach.
    //
    // A path that cannot be canonicalized is returned as it came. That is
    // almost always one that does not exist, which the caller has already
    // rejected — but a directory the process cannot open would land here too,
    // and for it the single-spelling guarantee is off. Opening the repository
    // and letting git report what is wrong beats refusing to show it at all,
    // and the cost of being wrong is the duplicate tab this exists to prevent,
    // not anything lost.
    crate::platform::paths::canonicalize_clean(candidate)
        .unwrap_or_else(|_| candidate.to_path_buf())
}

/// Format a `git2::Error` from `Repository::discover` for user-facing display.
///
/// When the error is a "not a repository" / `NotFound` error of class
/// `Repository`, the internal libgit2 diagnostic (`; class=Repository (6);
/// code=NotFound (-3)`) is stripped — users cannot act on it. All other
/// errors preserve the full `error.to_string()` so the diagnostic is
/// available for debugging.
pub fn format_discover_error(error: &git2::Error) -> String {
    if error.class() == git2::ErrorClass::Repository && error.code() == git2::ErrorCode::NotFound {
        error.message().to_string()
    } else {
        error.to_string()
    }
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

        // Compared literally. What libgit2 hands back is platform-specific — a
        // trailing separator, symlinks resolved on some systems and not others
        // — so this used to canonicalize both sides to compare structurally;
        // now the function does that itself, which is the point of it.
        assert_eq!(resolved, resolve_repo_path(&repo_path));
    }

    #[test]
    fn resolve_repo_path_keeps_non_repo_path() {
        // A path that does not exist cannot be canonicalized, so it is
        // returned as typed rather than being dropped.
        let path = PathBuf::from("/not/a/repo");

        assert_eq!(resolve_repo_path(&path), path);
    }

    #[test]
    fn every_spelling_of_one_worktree_resolves_alike() {
        // The de-duplication this exists for: a repository reached by a nested
        // directory, with a trailing separator, or through `.` is one tab.
        let (_dir, repo_path) = make_repo();
        let nested = Path::new(&repo_path).join("src");
        std::fs::create_dir(&nested).unwrap();
        let resolved = resolve_repo_path(&repo_path);

        assert_eq!(resolve_repo_path(&nested), resolved);
        assert_eq!(
            resolve_repo_path(PathBuf::from(format!("{repo_path}/"))),
            resolved
        );
        assert_eq!(resolve_repo_path(Path::new(&repo_path).join(".")), resolved);
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

    #[test]
    fn not_found_레포지토리_에러에서_class_code_가_제거된다() {
        let err = git2::Error::new(
            git2::ErrorCode::NotFound,
            git2::ErrorClass::Repository,
            "not a git repository: could not find repository at '/some/path'",
        );
        let formatted = format_discover_error(&err);
        assert!(
            !formatted.contains("class="),
            "should not contain class=: {formatted}"
        );
        assert!(
            !formatted.contains("code="),
            "should not contain code=: {formatted}"
        );
    }

    #[test]
    fn not_found_레포지토리_에러에_경로가_남아_있다() {
        let err = git2::Error::new(
            git2::ErrorCode::NotFound,
            git2::ErrorClass::Repository,
            "not a git repository: could not find repository at '/opt/data/.config/jcode'",
        );
        let formatted = format_discover_error(&err);
        assert!(
            formatted.contains("/opt/data/.config/jcode"),
            "should contain the path: {formatted}",
        );
    }

    #[test]
    fn 다른_에러_클래스는_전체_진단_정보를_유지한다() {
        // Auth + Repository: non-NotFound code, non-NotFound class.
        let err = git2::Error::new(
            git2::ErrorCode::Auth,
            git2::ErrorClass::Repository,
            "authentication failed",
        );
        let formatted = format_discover_error(&err);
        assert!(
            formatted.contains("class="),
            "should contain class=: {formatted}"
        );
        assert!(
            formatted.contains("code="),
            "should contain code=: {formatted}"
        );
    }

    #[test]
    fn not_found지만_레포지토리가_아닌_클래스는_전체_진단을_유지한다() {
        // NotFound + non-Repository class should keep the full diagnostic.
        let err = git2::Error::new(
            git2::ErrorCode::NotFound,
            git2::ErrorClass::Config,
            "config file not found: /etc/gitconfig",
        );
        let formatted = format_discover_error(&err);
        assert!(
            formatted.contains("class="),
            "should contain class=: {formatted}"
        );
        assert!(
            formatted.contains("code="),
            "should contain code=: {formatted}"
        );
    }
}
