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

#[test]
fn validate_commit_path_allows_a_deleted_worktree_path() {
    let (_dir, root) = workdir();

    // Historical diff routes must be able to address a file that no
    // longer exists in the current checkout.
    assert!(validate_commit_path("gone.txt").is_ok());
    assert!(resolve_in_workdir(&root, "gone.txt").is_err());
}

#[test]
fn validate_commit_path_keeps_the_worktree_safety_rules() {
    for path in [
        "../secret",
        "/etc/passwd",
        ".git/config",
        "src/../x",
        "x\0y",
    ] {
        assert!(validate_commit_path(path).is_err(), "{path:?} was accepted");
    }
}