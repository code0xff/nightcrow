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

#[cfg(unix)]
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

#[cfg(unix)]
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

#[cfg(windows)]
fn junction(link: &Path, target: &Path) -> bool {
    std::process::Command::new("cmd")
        .args(["/C", "mklink", "/J"])
        .arg(link)
        .arg(target)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .is_ok_and(|s| s.success())
}

#[cfg(windows)]
#[test]
fn resolve_in_workdir_rejects_a_junction_out_of_the_worktree() {
    let outside = tempfile::TempDir::new().unwrap();
    std::fs::write(outside.path().join("secrets.txt"), "token").unwrap();
    let (_dir, root) = workdir();
    let link = root.join("escape");
    if !junction(&link, outside.path()) {
        eprintln!("skipping junction test: mklink /J failed (FAT32 or missing privilege?)");
        return;
    }

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

/// Windows reads a component without its trailing dots and spaces, so a name
/// Rust parsed as ordinary can still be `..`.
#[test]
fn validate_commit_path_rejects_traversal_spelled_with_padding() {
    for attack in [".. /etc/passwd", "sub/.. /..", ".. ", "..."] {
        assert!(
            validate_commit_path(attack).is_err(),
            "accepted a traversal: {attack:?}"
        );
    }
}

/// `.git` has more than one spelling that opens it.
#[test]
fn validate_commit_path_rejects_every_spelling_of_the_git_dir() {
    for attack in [
        ".git/config",
        "GIT~1/config",
        "git~1/config",
        ".GIT./config",
    ] {
        assert!(
            validate_commit_path(attack).is_err(),
            "accepted the git directory: {attack:?}"
        );
    }
}

#[test]
fn validate_commit_path_still_accepts_an_ordinary_name() {
    for ok in [
        "src/main.rs",
        "a.b.c",
        "..hidden",
        "git~10/x",
        "my.git.notes",
    ] {
        assert!(validate_commit_path(ok).is_ok(), "refused: {ok:?}");
    }
}

/// A filesystem can hand back a different file than the name asked for.
///
/// Each of these is a documented rewrite — an NTFS alternate-stream suffix, the
/// code points HFS+ ignores, the trailing padding Windows drops — and each one
/// spells `.git` or `..` in a way a literal comparison does not see.
#[test]
fn validate_commit_path_judges_the_name_the_filesystem_opens() {
    for attack in [
        ".git::$INDEX_ALLOCATION/config",
        ".git:whatever/config",
        "\u{200c}.git/config",
        ".gi\u{200c}t/config",
        ".git\u{feff}/config",
        ".\u{200c}./etc/passwd",
        "..\u{200d}/.. /passwd",
    ] {
        assert!(
            validate_commit_path(attack).is_err(),
            "accepted a rewritten name: {attack:?}"
        );
    }
}

/// The rewrites must not swallow names that mean themselves.
#[test]
fn validate_commit_path_keeps_names_that_only_look_like_the_rules() {
    // `:f.rs` is the one that has to keep working: a stream suffix hangs off a
    // name, so a leading colon is not one, and git addresses the file fine.
    for ok in [
        "a:b.rs",
        "src/x:y",
        ":f.rs",
        "src/:odd",
        "gitignore~1",
        "..hidden/a\u{200c}b.rs",
    ] {
        assert!(validate_commit_path(ok).is_ok(), "refused: {ok:?}");
    }
}

/// Windows reads one letter before a colon as a drive, wherever the name sits.
///
/// `Path::components` only reports the prefix at the start of a path, so `c:x`
/// arrives as one ordinary component — and `PathBuf::push` parses it again and
/// replaces the buffer it was extending.
#[test]
#[cfg(windows)]
fn validate_commit_path_rejects_a_component_windows_reads_as_a_drive() {
    for attack in ["src/c:x", "src/c:/x", "a/b/z:secret"] {
        assert!(
            validate_commit_path(attack).is_err(),
            "accepted a drive-relative component: {attack:?}"
        );
    }
}
