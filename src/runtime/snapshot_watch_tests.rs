use super::{Roots, any_matters, external_git_dir};
use crate::test_util::{make_linked_worktree, make_repo, run_git};
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
fn an_ordinary_repository_needs_no_second_watch() {
    // `.git` is inside the tree, so the recursive watch already covers it and a
    // second one would only double every event.
    let (dir, path) = make_repo();
    let repo = crate::test_util::open_repo(&path);

    assert!(external_git_dir(&repo, &Roots::of(Path::new(&path))).is_none());
    drop(dir);
}

#[test]
fn a_linked_worktree_is_watched_where_its_state_lives() {
    // `git worktree add` leaves a `.git` *file* pointing at the main
    // repository's directory, and that is where the index and the refs are. A
    // watch on the tree alone would never see a `git add`.
    let (main, elsewhere, tree) = make_linked_worktree();
    let repo = crate::test_util::open_repo(&tree);

    let watched =
        external_git_dir(&repo, &Roots::of(Path::new(&tree))).expect("the git dir is elsewhere");

    assert!(
        watched.join("worktrees").is_dir(),
        "the main repository's git directory, which holds both trees' state: {}",
        watched.display()
    );
    drop((main, elsewhere));
}

#[test]
fn inside_a_git_directory_of_its_own_the_same_paths_matter() {
    // Once that second watch is installed its events arrive named after a
    // directory the tree filter cannot place, and "cannot place" means "read
    // it" — which would turn every loose object into a walk. The git-metadata
    // rules apply there instead.
    let (dir, path) = make_repo();
    let elsewhere = tempfile::TempDir::new().unwrap();
    let git_dir = elsewhere.path().to_string_lossy().to_string();
    let repo = crate::test_util::open_repo(&path);
    let mut roots = Roots::of(Path::new(&path));
    roots.set_external_git_dir(Some(Path::new(&git_dir)));

    for interesting in ["index", "HEAD", "refs/heads/main", "worktrees/wt/index"] {
        assert!(
            any_matters(Some(&repo), &roots, &under(&git_dir, interesting)),
            "{interesting} changes what a status says"
        );
    }
    for noise in [
        "objects/ab/cdef0123456789",
        "logs/HEAD",
        "worktrees/wt/index.lock",
    ] {
        assert!(
            !any_matters(Some(&repo), &roots, &under(&git_dir, noise)),
            "{noise} does not"
        );
    }
    drop(dir);
}

#[test]
fn a_submodules_own_object_churn_does_not_matter_either() {
    // A submodule keeps a git directory of its own under `modules/<name>/`, and
    // a fetch inside it writes the same loose objects and reflogs. Judged by the
    // top-level rule alone they read as `modules/...` — neither `objects` nor
    // `logs` — and every fetched object would cost a walk of the parent.
    let (dir, path) = make_repo();
    let elsewhere = tempfile::TempDir::new().unwrap();
    let git_dir = elsewhere.path().to_string_lossy().to_string();
    let repo = crate::test_util::open_repo(&path);
    let mut roots = Roots::of(Path::new(&path));
    roots.set_external_git_dir(Some(Path::new(&git_dir)));

    for noise in [
        "modules/sub/objects/ab/cdef",
        "modules/sub/logs/HEAD",
        "modules/outer/modules/inner/objects/ab/cdef",
    ] {
        assert!(
            !any_matters(Some(&repo), &roots, &under(&git_dir, noise)),
            "{noise} does not change the parent's status"
        );
    }
    for interesting in [
        "modules/sub/index",
        "modules/sub/HEAD",
        // A submodule may be called anything, including `objects`; only a name
        // *after* `modules` is stripped, so its index is still its index.
        "modules/objects/index",
    ] {
        assert!(
            any_matters(Some(&repo), &roots, &under(&git_dir, interesting)),
            "{interesting} does"
        );
    }
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
