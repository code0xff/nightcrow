use super::*;

#[test]
fn read_children_sorts_dirs_first_then_alpha() {
    let (dir, path) = make_repo();
    let root = StdPath::new(&path);
    std::fs::create_dir(root.join("zeta_dir")).unwrap();
    std::fs::create_dir(root.join("alpha_dir")).unwrap();
    std::fs::write(root.join("b_file.txt"), "x").unwrap();
    std::fs::write(root.join("a_file.txt"), "x").unwrap();

    let repo = open_repo(&path);
    let entries = read_children(&repo, root, "", true).unwrap();

    assert_eq!(
        names(&entries),
        vec!["alpha_dir", "zeta_dir", "a_file.txt", "b_file.txt"]
    );
    assert!(entries[..2].iter().all(|entry| entry.is_dir));
    assert!(entries[2..].iter().all(|entry| !entry.is_dir));
    drop(dir);
}

#[test]
fn read_children_reads_nested_dir_lazily() {
    let (dir, path) = make_repo();
    let root = StdPath::new(&path);
    std::fs::create_dir_all(root.join("src/ui")).unwrap();
    std::fs::write(root.join("src/main.rs"), "fn main() {}").unwrap();

    let repo = open_repo(&path);
    let entries = read_children(&repo, root, "src", true).unwrap();

    assert_eq!(names(&entries), vec!["ui", "main.rs"]);
    drop(dir);
}

#[test]
fn read_children_skips_git_metadata() {
    let (dir, path) = make_repo();
    let root = StdPath::new(&path);
    std::fs::write(root.join("a.txt"), "x").unwrap();

    let repo = open_repo(&path);
    let entries = read_children(&repo, root, "", true).unwrap();

    assert!(!names(&entries).contains(&".git"));
    assert!(names(&entries).contains(&"a.txt"));
    drop(dir);
}

#[test]
fn read_children_respects_gitignore_when_enabled() {
    let (dir, path) = make_repo();
    let root = StdPath::new(&path);
    std::fs::write(root.join(".gitignore"), "ignored.log\nbuild/\n").unwrap();
    std::fs::write(root.join("ignored.log"), "x").unwrap();
    std::fs::write(root.join("kept.rs"), "x").unwrap();
    std::fs::create_dir(root.join("build")).unwrap();
    run_git(&path, &["add", ".gitignore"]);
    run_git(&path, &["commit", "-m", "add gitignore"]);

    let repo = open_repo(&path);
    let filtered_entries = read_children(&repo, root, "", true).unwrap();
    let filtered = names(&filtered_entries);
    assert!(!filtered.contains(&"ignored.log"));
    assert!(!filtered.contains(&"build"));
    assert!(filtered.contains(&"kept.rs"));

    let unfiltered_entries = read_children(&repo, root, "", false).unwrap();
    let unfiltered = names(&unfiltered_entries);
    assert!(unfiltered.contains(&"ignored.log"));
    assert!(unfiltered.contains(&"build"));
    drop(dir);
}
