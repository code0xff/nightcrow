//! Names that a filesystem reads as something other than what was written.
//!
//! Each rule here exists because some filesystem rewrites a component before
//! opening it, so the name that was validated is not the name that gets used.

use crate::git::path::validate_commit_path;

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
        // Not `..\u{200d}/.. /passwd`: the trailing space on the second
        // component refuses that one on its own, so the row would stay green
        // with the ignorable-character rule deleted.
        "..\u{200d}/passwd",
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
        // Not `a:b.rs`: Windows reads one letter before a colon as a drive, so
        // that name is a path with a prefix there, not a file — and this list
        // has to hold on all three platforms.
        "ab:c.rs",
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
