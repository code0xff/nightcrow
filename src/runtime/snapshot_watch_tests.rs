use super::{Roots, any_matters};
use crate::test_util::{make_repo, run_git};
use std::path::{Path, PathBuf};

fn under(root: &str, relative: &str) -> Vec<PathBuf> {
    vec![Path::new(root).join(relative)]
}

#[test]
fn a_source_file_matters() {
    let (dir, path) = make_repo();
    let repo = crate::test_util::open_repo(&path);

    assert!(any_matters(
        Some(&repo),
        &Roots::of(Path::new(&path)),
        &under(&path, "src/main.rs")
    ));
    drop(dir);
}

#[test]
fn an_ignored_path_does_not() {
    // The reason this filter exists: a build writes thousands of files git has
    // been told to disregard, and none of them can appear in a status.
    let (dir, path) = make_repo();
    std::fs::write(Path::new(&path).join(".gitignore"), "target/\n*.log\n").unwrap();
    let repo = crate::test_util::open_repo(&path);

    assert!(!any_matters(
        Some(&repo),
        &Roots::of(Path::new(&path)),
        &under(&path, "target/debug/build/x.o")
    ));
    assert!(!any_matters(
        Some(&repo),
        &Roots::of(Path::new(&path)),
        &under(&path, "build.log")
    ));
    drop(dir);
}

#[test]
fn the_index_and_refs_matter_but_objects_and_reflogs_do_not() {
    // A commit or a fetch writes objects and reflogs as well as the ref that
    // names them; the ref and the index are what change a status, and reading on
    // the object churn instead would walk the tree once per loose object.
    let (dir, path) = make_repo();
    let repo = crate::test_util::open_repo(&path);
    let roots = Roots::of(Path::new(&path));

    for interesting in [
        ".git/index",
        ".git/HEAD",
        ".git/refs/heads/main",
        ".git/packed-refs",
    ] {
        assert!(
            any_matters(Some(&repo), &roots, &under(&path, interesting)),
            "{interesting} changes what a status says"
        );
    }
    for noise in [
        ".git/objects/ab/cdef0123456789",
        ".git/logs/HEAD",
        ".git/index.lock",
    ] {
        assert!(
            !any_matters(Some(&repo), &roots, &under(&path, noise)),
            "{noise} does not"
        );
    }
    drop(dir);
}

#[test]
fn a_change_the_watcher_could_not_name_matters() {
    // An empty list is what a watcher error becomes: it may have missed events,
    // so the answer is to look rather than to decide there was nothing.
    let (dir, path) = make_repo();
    let repo = crate::test_util::open_repo(&path);

    assert!(any_matters(Some(&repo), &Roots::of(Path::new(&path)), &[]));
    drop(dir);
}

#[test]
fn without_a_repository_handle_everything_matters() {
    // Nothing to ask about ignore rules yet, and guessing the other way would
    // leave a real change unread.
    let (dir, path) = make_repo();

    assert!(any_matters(
        None,
        &Roots::of(Path::new(&path)),
        &under(&path, "any.rs")
    ));
    drop(dir);
}

#[test]
fn a_path_outside_the_tree_matters() {
    // The watcher should not report these. One that cannot be placed is read
    // rather than dropped.
    let (dir, path) = make_repo();
    let repo = crate::test_util::open_repo(&path);
    run_git(&path, &["status"]);

    assert!(any_matters(
        Some(&repo),
        &Roots::of(Path::new(&path)),
        &[PathBuf::from("/elsewhere/file.rs")]
    ));
    drop(dir);
}
