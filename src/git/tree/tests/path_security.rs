use super::*;

#[cfg(unix)]
#[test]
fn read_children_refuses_to_list_the_git_directory() {
    let (dir, path) = make_repo();
    let root = StdPath::new(&path);
    let repo = open_repo(&path);

    for asked in [".git", ".git/refs", ".GIT", "src/../.git"] {
        let err = read_children(&repo, root, asked, false).unwrap_err();
        assert!(
            !err.to_string().contains("failed to read directory"),
            "{asked:?} must be rejected by validation: {err}"
        );
    }
    drop(dir);
}

#[test]
fn read_children_refuses_traversal_that_stays_inside_the_worktree() {
    let (dir, path) = make_repo();
    let root = StdPath::new(&path);
    std::fs::create_dir(root.join("src")).unwrap();
    let repo = open_repo(&path);

    let err = read_children(&repo, root, "src/..", false).unwrap_err();

    assert!(err.to_string().contains("plain relative path"), "{err}");
    drop(dir);
}

#[cfg(unix)]
#[test]
fn read_children_refuses_to_descend_a_symlinked_directory() {
    let (dir, path) = make_repo();
    let root = StdPath::new(&path);
    std::fs::create_dir(root.join("real_dir")).unwrap();
    std::fs::write(root.join("real_dir/secret.txt"), "x").unwrap();
    std::os::unix::fs::symlink(root.join("real_dir"), root.join("link_dir")).unwrap();

    let repo = open_repo(&path);
    let err = read_children(&repo, root, "link_dir", false).unwrap_err();
    assert!(
        err.to_string().contains("symlinks are not followed"),
        "{err}"
    );
    assert!(read_children(&repo, root, "real_dir", false).is_ok());
    drop(dir);
}

#[cfg(windows)]
fn junction(link: &StdPath, target: &StdPath) -> bool {
    std::process::Command::new("cmd")
        .args(["/C", "mklink", "/J"])
        .arg(link)
        .arg(target)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .is_ok_and(|status| status.success())
}

#[cfg(windows)]
#[test]
fn read_children_refuses_to_descend_a_junction() {
    let (dir, path) = make_repo();
    let root = StdPath::new(&path);
    std::fs::create_dir(root.join("real_dir")).unwrap();
    std::fs::write(root.join("real_dir/secret.txt"), "x").unwrap();
    let link = root.join("link_dir");
    if !junction(&link, &root.join("real_dir")) {
        eprintln!("skipping junction test: mklink /J failed");
        return;
    }

    let repo = open_repo(&path);
    let err = read_children(&repo, root, "link_dir", false).unwrap_err();
    assert!(
        err.to_string().contains("symlinks are not followed"),
        "{err}"
    );
    assert!(read_children(&repo, root, "real_dir", false).is_ok());
    drop(dir);
}

#[cfg(unix)]
#[test]
fn read_children_rejects_escape_through_symlinked_parent() {
    let (dir, path) = make_repo();
    let root = StdPath::new(&path);
    let outside = tempfile::TempDir::new().unwrap();
    std::fs::create_dir(outside.path().join("sub")).unwrap();
    std::fs::write(outside.path().join("sub/secret.txt"), "x").unwrap();
    std::os::unix::fs::symlink(outside.path(), root.join("link")).unwrap();

    let repo = open_repo(&path);
    let err = read_children(&repo, root, "link/sub", false).unwrap_err();
    assert!(
        err.to_string().contains("symlinks are not followed"),
        "{err}"
    );
    drop(dir);
}

#[cfg(unix)]
#[test]
fn read_children_reports_symlinked_dir_as_non_dir() {
    let (dir, path) = make_repo();
    let root = StdPath::new(&path);
    std::fs::create_dir(root.join("real_dir")).unwrap();
    std::os::unix::fs::symlink(root.join("real_dir"), root.join("link_dir")).unwrap();

    let repo = open_repo(&path);
    let entries = read_children(&repo, root, "", false).unwrap();

    let link = entries
        .iter()
        .find(|entry| entry.name == "link_dir")
        .unwrap();
    let real = entries
        .iter()
        .find(|entry| entry.name == "real_dir")
        .unwrap();
    assert!(!link.is_dir);
    assert!(real.is_dir);
    drop(dir);
}

/// What the listing offers must be what the gate opens.
///
/// A `...` directory is a name Windows drops the padding from, so the path
/// validator refuses it — and the tree used to list it anyway. Clicking the row
/// answered "not a plain relative path", and a search for anything beneath it
/// came back empty with no sign the subtree existed.
///
/// Unix-only because Windows cannot create the directory this is about.
#[test]
#[cfg(unix)]
fn a_name_the_path_gate_refuses_is_not_listed_or_searched() {
    let (dir, path) = make_repo();
    let root = StdPath::new(&path);
    std::fs::create_dir(root.join("...")).unwrap();
    std::fs::write(root.join("...").join("deep.rs"), "fn deep() {}\n").unwrap();
    std::fs::create_dir(root.join("kept")).unwrap();
    std::fs::write(root.join("kept").join("deep.rs"), "fn deep() {}\n").unwrap();
    run_git(&path, &["add", "-A"]);
    run_git(&path, &["commit", "-m", "add both"]);

    let repo = open_repo(&path);
    let entries = read_children(&repo, root, "", false).unwrap();
    assert!(!names(&entries).contains(&"..."), "{:?}", names(&entries));
    assert!(names(&entries).contains(&"kept"));

    let (found, _) = search_tree(&repo, root, "deep", 64, 10_000, 100).unwrap();
    assert_eq!(paths(&found), vec!["kept/deep.rs"]);
    drop(dir);
}
