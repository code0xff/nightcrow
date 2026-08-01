use super::*;

#[test]
fn an_ordinary_repository_needs_no_second_watch() {
    let (dir, path) = make_repo();
    let repo = crate::test_util::open_repo(&path);

    assert!(external_git_dir(&repo, &Roots::of(Path::new(&path))).is_none());
    drop(dir);
}

#[test]
fn a_root_spelled_canonically_still_places_a_plainly_spelled_path() {
    let (dir, path) = make_repo();
    let canonical = std::fs::canonicalize(&path).expect("the repo exists");
    let clean = crate::platform::paths::canonicalize_clean(&path).expect("the repo exists");

    assert!(
        Roots::of(&canonical)
            .tree
            .relative(&clean.join("src/main.rs"))
            .is_some()
    );
    drop(dir);
}

#[test]
fn a_linked_worktree_is_watched_where_its_state_lives() {
    let (main, elsewhere, tree) = make_linked_worktree();
    let repo = crate::test_util::open_repo(&tree);

    let watched = external_git_dir(&repo, &Roots::of(Path::new(&tree))).expect("external git dir");

    assert!(watched.join("worktrees").is_dir(), "{}", watched.display());
    drop((main, elsewhere));
}

#[test]
fn inside_an_external_git_directory_the_same_paths_matter() {
    let (dir, path) = make_repo();
    let elsewhere = tempfile::TempDir::new().unwrap();
    let git_dir = elsewhere.path().to_string_lossy().to_string();
    let repo = crate::test_util::open_repo(&path);
    let mut roots = Roots::of(Path::new(&path));
    roots.set_external_git_dir(Some(Path::new(&git_dir)));

    for interesting in ["index", "HEAD", "refs/heads/main", "worktrees/wt/index"] {
        assert!(any_matters(
            Some(&repo),
            &roots,
            &under(&git_dir, interesting)
        ));
    }
    for noise in [
        "objects/ab/cdef0123456789",
        "logs/HEAD",
        "worktrees/wt/index.lock",
    ] {
        assert!(!any_matters(Some(&repo), &roots, &under(&git_dir, noise)));
    }
    drop(dir);
}

#[test]
fn a_submodules_git_directory_is_read_rather_than_judged() {
    let (dir, path) = make_repo();
    let elsewhere = tempfile::TempDir::new().unwrap();
    let git_dir = elsewhere.path().to_string_lossy().to_string();
    let repo = crate::test_util::open_repo(&path);
    let mut roots = Roots::of(Path::new(&path));
    roots.set_external_git_dir(Some(Path::new(&git_dir)));

    for path in [
        "modules/sub/objects/ab/cdef",
        "modules/sub/HEAD",
        "modules/foo/objects/HEAD",
    ] {
        assert!(any_matters(Some(&repo), &roots, &under(&git_dir, path)));
    }
    drop(dir);
}
