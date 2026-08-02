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

/// A directory is a prefix to git, and a prefix is not what was asked for.
///
/// `disable_pathspec_match` turns off globbing but not prefix matching, so `src`
/// used to answer with every changed file beneath it — under the label `src`,
/// as though one file had all those hunks. The routes cannot tell a directory
/// from a file without going to disk, and going to disk is what refused a
/// deleted file's diff, so the over-collected deltas are dropped instead.
#[test]
fn a_directory_is_not_a_file_and_answers_with_nothing() {
    let (dir, path) = make_repo();
    std::fs::create_dir(Path::new(&path).join("src")).unwrap();
    for name in ["a.rs", "b.rs"] {
        std::fs::write(Path::new(&path).join("src").join(name), "fn x() {}\n").unwrap();
    }
    run_git(&path, &["add", "."]);
    run_git(&path, &["commit", "-m", "init"]);
    for name in ["a.rs", "b.rs"] {
        std::fs::write(Path::new(&path).join("src").join(name), "fn x() { y(); }\n").unwrap();
    }
    run_git(&path, &["add", "-A"]);
    run_git(&path, &["commit", "-m", "change both"]);

    let repo = open_repo(&path);
    let oid = load_commit_log(&repo, 1).unwrap()[0].oid;
    assert!(
        !load_commit_file_diff(&repo, oid, "src/a.rs")
            .unwrap()
            .is_empty()
    );
    for directory in ["src", "src/"] {
        assert!(
            load_commit_file_diff(&repo, oid, directory)
                .unwrap()
                .is_empty(),
            "a directory answered as if it were a file: {directory}"
        );
    }
    drop(dir);
}

/// A moved file answers under either of its names.
///
/// The contract, not the mechanism: a commit's file list carries the old name
/// beside the new, and narrowing the diff to the requested path must not make
/// one of them answer with nothing. Today each name arrives as its own delta —
/// git pairs a rename only when nothing narrowed the diff — so this passes
/// whichever side the filter reads; it is here to keep it passing if that
/// changes.
#[test]
fn a_renamed_file_answers_to_both_of_its_names() {
    let (dir, path) = make_repo();
    std::fs::write(Path::new(&path).join("before.rs"), "fn x() {}\n").unwrap();
    run_git(&path, &["add", "."]);
    run_git(&path, &["commit", "-m", "init"]);
    // A pure move, so git records one rename delta. Editing the file too would
    // leave a delete plus an add, whose delta still carries `before.rs` as its
    // own path — and the test would pass without matching old names at all.
    run_git(&path, &["mv", "before.rs", "after.rs"]);
    run_git(&path, &["add", "-A"]);
    run_git(&path, &["commit", "-m", "rename"]);

    let repo = open_repo(&path);
    let oid = load_commit_log(&repo, 1).unwrap()[0].oid;
    for name in ["after.rs", "before.rs"] {
        assert!(
            !load_commit_file_diff(&repo, oid, name).unwrap().is_empty(),
            "a rename stopped answering to {name}"
        );
    }
    drop(dir);
}
