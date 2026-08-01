use super::*;

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
    let (dir, path) = make_repo();
    std::fs::write(Path::new(&path).join(".gitignore"), "target/\n*.log\n").unwrap();
    let repo = crate::test_util::open_repo(&path);
    let roots = Roots::of(Path::new(&path));

    for ignored in ["target/debug/build/x.o", "build.log"] {
        assert!(!any_matters(Some(&repo), &roots, &under(&path, ignored)));
    }
    drop(dir);
}

#[test]
fn the_index_and_refs_matter_but_objects_and_reflogs_do_not() {
    let (dir, path) = make_repo();
    let repo = crate::test_util::open_repo(&path);
    let roots = Roots::of(Path::new(&path));

    for interesting in [
        ".git/index",
        ".git/HEAD",
        ".git/refs/heads/main",
        ".git/packed-refs",
    ] {
        assert!(any_matters(Some(&repo), &roots, &under(&path, interesting)));
    }
    for noise in [
        ".git/objects/ab/cdef0123456789",
        ".git/logs/HEAD",
        ".git/index.lock",
    ] {
        assert!(!any_matters(Some(&repo), &roots, &under(&path, noise)));
    }
    drop(dir);
}

#[test]
fn a_change_the_watcher_could_not_name_matters() {
    let (dir, path) = make_repo();
    let repo = crate::test_util::open_repo(&path);

    assert!(any_matters(Some(&repo), &Roots::of(Path::new(&path)), &[]));
    drop(dir);
}

#[test]
fn without_a_repository_handle_everything_matters() {
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
    let (dir, path) = make_repo();
    let repo = crate::test_util::open_repo(&path);

    assert!(any_matters(
        Some(&repo),
        &Roots::of(Path::new(&path)),
        &[PathBuf::from("/elsewhere/file.rs")]
    ));
    drop(dir);
}
