use super::{Roots, any_matters, changed_paths, external_git_dir};
use crate::test_util::{make_linked_worktree, make_repo, run_git};
use notify::event::{AccessKind, AccessMode, CreateKind, Event, EventKind, Flag, ModifyKind};
use std::path::{Path, PathBuf};

fn under(root: &str, relative: &str) -> Vec<PathBuf> {
    vec![Path::new(root).join(relative)]
}

/// The whole gate the watcher applies: the kind decides whether the event is
/// forwarded at all, and the paths then decide whether it is worth a read.
fn wakes_the_reader(root: &str, event: notify::Result<Event>) -> bool {
    let repo = crate::test_util::open_repo(root);
    changed_paths(event)
        .is_some_and(|paths| any_matters(Some(&repo), &Roots::of(Path::new(root)), &paths))
}

fn at(kind: EventKind, root: &str, relative: &str) -> notify::Result<Event> {
    Ok(Event::new(kind).add_path(Path::new(root).join(relative)))
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
fn a_submodules_git_directory_is_read_rather_than_judged() {
    // A submodule keeps one under `modules/<name>/`, where the same objects and
    // reflogs churn — but a submodule's name is its path in the tree, slashes
    // included, so `modules/foo/objects/HEAD` is the `HEAD` of a submodule at
    // `foo/objects` just as readily as the object store of one at `foo`. The
    // rule stays at the top level rather than guess: over-reading costs a walk
    // per second during a fetch, dropping the wrong one costs a change nobody
    // sees.
    let (dir, path) = make_repo();
    let elsewhere = tempfile::TempDir::new().unwrap();
    let git_dir = elsewhere.path().to_string_lossy().to_string();
    let repo = crate::test_util::open_repo(&path);
    let mut roots = Roots::of(Path::new(&path));
    roots.set_external_git_dir(Some(Path::new(&git_dir)));

    for under_a_submodule in [
        "modules/sub/objects/ab/cdef",
        "modules/sub/HEAD",
        "modules/foo/objects/HEAD",
    ] {
        assert!(
            any_matters(Some(&repo), &roots, &under(&git_dir, under_a_submodule)),
            "{under_a_submodule} is read rather than judged"
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

#[test]
fn an_open_caused_by_the_read_itself_does_not_wake_the_reader() {
    // The loop this closes. A status read opens `HEAD`, the branch ref and the
    // work-tree root, inotify reports each of those as an open, and every one of
    // them clears the filter above — so the reader kept re-reading at the rate
    // limit for as long as a repository was on screen.
    let (dir, path) = make_repo();

    for opened in [".git/HEAD", ".git/refs/heads/main", "src/main.rs", ""] {
        let event = at(
            EventKind::Access(AccessKind::Open(AccessMode::Any)),
            &path,
            opened,
        );
        assert!(
            !wakes_the_reader(&path, event),
            "opening {opened} is a read, not a change"
        );
    }
    drop(dir);
}

#[test]
fn a_finished_write_wakes_the_reader() {
    // `IN_CLOSE_WRITE` is the one access that is not somebody looking, so it
    // cannot go with the rest: dropping it would lose real changes on Linux.
    let (dir, path) = make_repo();
    let closed = EventKind::Access(AccessKind::Close(AccessMode::Write));

    assert!(wakes_the_reader(&path, at(closed, &path, ".git/HEAD")));
    assert!(wakes_the_reader(&path, at(closed, &path, "src/main.rs")));
    drop(dir);
}

#[test]
fn an_ordinary_write_or_creation_wakes_the_reader() {
    // Nothing outside `Access` was touched; this is what says so.
    let (dir, path) = make_repo();

    for kind in [
        EventKind::Modify(ModifyKind::Any),
        EventKind::Create(CreateKind::File),
    ] {
        assert!(
            wakes_the_reader(&path, at(kind, &path, "src/main.rs")),
            "{kind:?} changes what a status says"
        );
    }
    drop(dir);
}

#[test]
fn a_dropped_events_signal_wakes_the_reader() {
    // An inotify queue overflow arrives as `Other` with the rescan flag and no
    // paths at all, and a watcher error names nothing either. Both mean events
    // were missed, so both must still reach the worker.
    let (dir, path) = make_repo();

    let overflowed = Ok(Event::new(EventKind::Other).set_flag(Flag::Rescan));
    assert!(wakes_the_reader(&path, overflowed));
    assert!(wakes_the_reader(&path, Err(notify::Error::generic("boom"))));
    drop(dir);
}
