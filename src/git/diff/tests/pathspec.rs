//! What a caller's path string means to git when a route hands it over.

use crate::git::diff::{load_commit_file_diff, load_commit_log, load_file_diff};
use crate::test_util::{make_repo, open_repo, run_git};
use std::path::Path;

/// A path is a path here, never a pattern.
///
/// `diff_options` sets `disable_pathspec_match`, so the string names one file
/// rather than being matched as a glob. That is what lets a route hand a
/// caller's string straight to git and still answer about the file it was asked
/// about: without the flag, `*.rs` returns every Rust file's changes under the
/// name the caller supplied. Pinned here because it is one option away.
///
/// Git's own pathspec magic (`:(glob)`, `:(exclude)`, `:/`) is in the list for
/// completeness only — libgit2 does not implement it, so those strings are just
/// filenames nothing is called, with the flag or without it.
#[test]
fn a_pathspec_is_matched_literally_and_never_as_magic() {
    let (dir, path) = make_repo();
    let file = Path::new(&path).join("b.rs");
    std::fs::write(&file, "fn b() {}\n").unwrap();
    run_git(&path, &["add", "."]);
    run_git(&path, &["commit", "-m", "add b"]);
    std::fs::write(&file, "fn b() { changed(); }\n").unwrap();

    let repo = open_repo(&path);
    let oid = load_commit_log(&repo, 1).unwrap()[0].oid;
    assert!(!load_file_diff(&repo, "b.rs").unwrap().is_empty());
    assert!(
        !load_commit_file_diff(&repo, oid, "b.rs")
            .unwrap()
            .is_empty()
    );

    for magic in ["*.rs", "b.*", "*", ":(glob)**", ":/"] {
        assert!(
            load_file_diff(&repo, magic).unwrap().is_empty(),
            "magic matched a working-tree change: {magic}"
        );
        assert!(
            load_commit_file_diff(&repo, oid, magic).unwrap().is_empty(),
            "magic matched a commit change: {magic}"
        );
    }
    drop(dir);
}
