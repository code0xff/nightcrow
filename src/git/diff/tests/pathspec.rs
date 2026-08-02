//! What a caller's path string means to git when a route hands it over.

use crate::git::diff::{load_commit_file_diff, load_commit_log, load_file_diff};
use crate::test_util::{make_repo, open_repo, run_git};
use std::path::Path;

/// A path is a path here, never a pattern.
///
/// A route hands a caller's string straight to git and must still answer about
/// the file it was asked about, so `*.rs` names a file called `*.rs` and
/// nothing else. Two things hold that now — `disable_pathspec_match` stops the
/// glob from matching, and the narrowing in `collect_hunks` drops whatever a
/// match would have collected — and this test passes with either one removed.
/// It pins the contract, not either mechanism; the flag still earns its place
/// by keeping the work undone rather than done and discarded.
///
/// Git's own pathspec magic (`:(glob)`, `:(exclude)`, `:/`) is in the list for
/// completeness only — libgit2 does not implement it, so those strings are just
/// filenames nothing is called, either way.
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

    // And again in the working tree, so both loaders are covered: the status
    // pane reads `load_file_diff` and the log drill-down `load_commit_file_diff`,
    // and each narrows a diff of its own.
    for name in ["a.rs", "b.rs"] {
        std::fs::write(Path::new(&path).join("src").join(name), "fn x() { z(); }\n").unwrap();
    }

    let repo = open_repo(&path);
    let oid = load_commit_log(&repo, 1).unwrap()[0].oid;
    assert!(
        !load_commit_file_diff(&repo, oid, "src/a.rs")
            .unwrap()
            .is_empty()
    );
    assert!(!load_file_diff(&repo, "src/a.rs").unwrap().is_empty());
    for directory in ["src", "src/"] {
        assert!(
            load_commit_file_diff(&repo, oid, directory)
                .unwrap()
                .is_empty(),
            "a commit directory answered as if it were a file: {directory}"
        );
        assert!(
            load_file_diff(&repo, directory).unwrap().is_empty(),
            "a worktree directory answered as if it were a file: {directory}"
        );
    }
    drop(dir);
}

/// A file that became a directory still answers under its own name.
///
/// `foo` moved to `foo/x` is one delta, and git pairs it as a rename because
/// the pathspec `foo` reaches both halves by prefix. Only the *old* side equals
/// what was asked for, so narrowing on the new side alone answers nothing about
/// a file the caller can plainly see in the commit's list.
#[test]
fn a_file_replaced_by_a_directory_of_its_name_still_answers() {
    let (dir, path) = make_repo();
    std::fs::write(Path::new(&path).join("foo"), "hello\n").unwrap();
    run_git(&path, &["add", "."]);
    run_git(&path, &["commit", "-m", "init"]);
    std::fs::remove_file(Path::new(&path).join("foo")).unwrap();
    std::fs::create_dir(Path::new(&path).join("foo")).unwrap();
    std::fs::write(Path::new(&path).join("foo/x"), "hello\n").unwrap();
    run_git(&path, &["add", "-A"]);
    run_git(&path, &["commit", "-m", "foo becomes a directory"]);

    let repo = open_repo(&path);
    let oid = load_commit_log(&repo, 1).unwrap()[0].oid;
    assert!(
        !load_commit_file_diff(&repo, oid, "foo").unwrap().is_empty(),
        "the file the commit moved away answered with nothing"
    );
    drop(dir);
}

/// And the mirror: a directory replaced by a file of its name.
///
/// `bar/x` moved to `bar` pairs the same way under the pathspec `bar`, with the
/// *new* side the only one equal to the request. Both arms of the match are
/// load-bearing, each for one of these two shapes.
#[test]
fn a_directory_replaced_by_a_file_of_its_name_still_answers() {
    let (dir, path) = make_repo();
    std::fs::create_dir(Path::new(&path).join("bar")).unwrap();
    std::fs::write(Path::new(&path).join("bar/x"), "hello\n").unwrap();
    run_git(&path, &["add", "."]);
    run_git(&path, &["commit", "-m", "init"]);
    std::fs::remove_dir_all(Path::new(&path).join("bar")).unwrap();
    std::fs::write(Path::new(&path).join("bar"), "hello\n").unwrap();
    run_git(&path, &["add", "-A"]);
    run_git(&path, &["commit", "-m", "bar becomes a file"]);

    let repo = open_repo(&path);
    let oid = load_commit_log(&repo, 1).unwrap()[0].oid;
    assert!(
        !load_commit_file_diff(&repo, oid, "bar").unwrap().is_empty(),
        "the file the commit moved into place answered with nothing"
    );
    drop(dir);
}

/// A moved file answers under either of its names.
///
/// The contract, not the mechanism: a commit's file list carries the old name
/// beside the new, and narrowing the diff to the requested path must not make
/// one of them answer with nothing. Here the pathspec reaches one half only, so
/// git leaves the halves unpaired and each carries its own name on both sides —
/// which is why this passes whichever side the filter reads, and why the case
/// above is the one that needs both.
#[test]
fn a_renamed_file_answers_to_both_of_its_names() {
    let (dir, path) = make_repo();
    std::fs::write(Path::new(&path).join("before.rs"), "fn x() {}\n").unwrap();
    run_git(&path, &["add", "."]);
    run_git(&path, &["commit", "-m", "init"]);
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
