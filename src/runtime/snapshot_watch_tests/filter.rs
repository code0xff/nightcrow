use super::*;

#[test]
fn a_source_file_matters() {
    let (dir, path) = make_repo();
    let repo = crate::test_util::open_repo(&path);

    assert!(any_matters_now(
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
    let mut ignored = IgnoredDirs::default();

    for path_under_test in ["target/debug/build/x.o", "build.log"] {
        assert!(!any_matters(
            Some(&repo),
            &roots,
            &under(&path, path_under_test),
            &mut ignored
        ));
    }
    drop(dir);
}

#[test]
fn an_ignored_directory_is_remembered_for_the_rest_of_the_burst() {
    let (dir, path) = make_repo();
    std::fs::write(Path::new(&path).join(".gitignore"), "target/\n").unwrap();
    let repo = crate::test_util::open_repo(&path);
    let roots = Roots::of(Path::new(&path));
    let mut ignored = IgnoredDirs::default();

    assert!(!any_matters(
        Some(&repo),
        &roots,
        &under(&path, "target/debug/build/x.o"),
        &mut ignored
    ));
    assert!(
        ignored.covers(Path::new("target/debug")),
        "the directory that explains the path must be remembered"
    );
    assert!(!any_matters(
        Some(&repo),
        &roots,
        &under(&path, "target/release/y.o"),
        &mut ignored
    ));
    drop(dir);
}

#[test]
fn a_changed_tracked_path_still_matters_after_an_ignored_directory_is_remembered() {
    let (dir, path) = make_repo();
    std::fs::write(Path::new(&path).join(".gitignore"), "target/\n").unwrap();
    let repo = crate::test_util::open_repo(&path);
    let roots = Roots::of(Path::new(&path));
    let mut ignored = IgnoredDirs::default();
    assert!(!any_matters(
        Some(&repo),
        &roots,
        &under(&path, "target/debug/build/x.o"),
        &mut ignored
    ));

    assert!(any_matters(
        Some(&repo),
        &roots,
        &under(&path, "src/main.rs"),
        &mut ignored
    ));
    drop(dir);
}

#[test]
fn a_file_ignored_by_name_does_not_hide_its_siblings() {
    let (dir, path) = make_repo();
    std::fs::write(Path::new(&path).join(".gitignore"), "*.log\n").unwrap();
    let repo = crate::test_util::open_repo(&path);
    let roots = Roots::of(Path::new(&path));
    let mut ignored = IgnoredDirs::default();

    assert!(!any_matters(
        Some(&repo),
        &roots,
        &under(&path, "build.log"),
        &mut ignored
    ));
    assert!(any_matters(
        Some(&repo),
        &roots,
        &under(&path, "notes.txt"),
        &mut ignored
    ));
    drop(dir);
}

#[test]
fn a_changed_ignore_file_forgets_the_remembered_directories() {
    let (dir, path) = make_repo();
    std::fs::write(Path::new(&path).join(".gitignore"), "target/\n").unwrap();
    let repo = crate::test_util::open_repo(&path);
    let roots = Roots::of(Path::new(&path));
    let mut ignored = IgnoredDirs::default();
    assert!(!any_matters(
        Some(&repo),
        &roots,
        &under(&path, "target/debug/build/x.o"),
        &mut ignored
    ));
    assert!(ignored.covers(Path::new("target/debug")));

    std::fs::write(Path::new(&path).join(".gitignore"), "other/\n").unwrap();
    assert!(any_matters(
        Some(&repo),
        &roots,
        &under(&path, ".gitignore"),
        &mut ignored
    ));

    assert!(
        !ignored.covers(Path::new("target/debug")),
        "a rules file in the batch must invalidate what the old rules said"
    );
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
        assert!(any_matters_now(
            Some(&repo),
            &roots,
            &under(&path, interesting)
        ));
    }
    for noise in [
        ".git/objects/ab/cdef0123456789",
        ".git/logs/HEAD",
        ".git/index.lock",
    ] {
        assert!(!any_matters_now(Some(&repo), &roots, &under(&path, noise)));
    }
    drop(dir);
}

#[test]
fn a_change_the_watcher_could_not_name_matters() {
    let (dir, path) = make_repo();
    let repo = crate::test_util::open_repo(&path);

    assert!(any_matters_now(
        Some(&repo),
        &Roots::of(Path::new(&path)),
        &[]
    ));
    drop(dir);
}

#[test]
fn without_a_repository_handle_everything_matters() {
    let (dir, path) = make_repo();

    assert!(any_matters_now(
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

    assert!(any_matters_now(
        Some(&repo),
        &Roots::of(Path::new(&path)),
        &[PathBuf::from("/elsewhere/file.rs")]
    ));
    drop(dir);
}
