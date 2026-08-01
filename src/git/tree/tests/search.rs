use super::*;

#[test]
fn search_tree_finds_nested_matches_by_basename() {
    let (dir, path) = make_repo();
    let root = StdPath::new(&path);
    std::fs::create_dir_all(root.join("src/ui")).unwrap();
    std::fs::write(root.join("src/main.rs"), "x").unwrap();
    std::fs::write(root.join("src/ui/tree_view.rs"), "x").unwrap();
    std::fs::write(root.join("README.md"), "x").unwrap();

    let repo = open_repo(&path);
    let (matches, truncated) = search_tree(&repo, root, "RS", 64, 10_000, 100).unwrap();
    assert_eq!(paths(&matches), vec!["src/main.rs", "src/ui/tree_view.rs"]);
    assert!(!truncated);
    drop(dir);
}

#[test]
fn search_tree_excludes_gitignored_and_git_metadata() {
    let (dir, path) = make_repo();
    let root = StdPath::new(&path);
    std::fs::write(root.join(".gitignore"), "target/\n").unwrap();
    std::fs::create_dir(root.join("target")).unwrap();
    std::fs::write(root.join("target/build.rs"), "x").unwrap();
    std::fs::write(root.join("keep.rs"), "x").unwrap();
    run_git(&path, &["add", ".gitignore"]);
    run_git(&path, &["commit", "-m", "add gitignore"]);

    let repo = open_repo(&path);
    let (matches, _) = search_tree(&repo, root, ".rs", 64, 10_000, 100).unwrap();
    assert_eq!(paths(&matches), vec!["keep.rs"]);
    drop(dir);
}

#[test]
fn search_tree_stops_descending_beyond_max_depth() {
    let (dir, path) = make_repo();
    let root = StdPath::new(&path);
    std::fs::create_dir_all(root.join("a/b")).unwrap();
    std::fs::write(root.join("a/shallow.txt"), "x").unwrap();
    std::fs::write(root.join("a/b/deep.txt"), "x").unwrap();

    let repo = open_repo(&path);
    let (matches, _) = search_tree(&repo, root, ".txt", 1, 10_000, 100).unwrap();
    assert_eq!(paths(&matches), vec!["a/shallow.txt"]);
    drop(dir);
}

#[test]
fn search_tree_caps_results_and_flags_truncation() {
    let (dir, path) = make_repo();
    let root = StdPath::new(&path);
    for i in 0..5 {
        std::fs::write(root.join(format!("hit_{i}.txt")), "x").unwrap();
    }

    let repo = open_repo(&path);
    let (matches, truncated) = search_tree(&repo, root, "hit_", 64, 10_000, 3).unwrap();
    assert_eq!(paths(&matches), vec!["hit_0.txt", "hit_1.txt", "hit_2.txt"]);
    assert!(truncated);
    drop(dir);
}

#[test]
fn search_tree_stops_when_the_visit_budget_is_exhausted() {
    let (dir, path) = make_repo();
    let root = StdPath::new(&path);
    for i in 0..20 {
        std::fs::write(root.join(format!("f_{i:02}.txt")), "x").unwrap();
    }

    let repo = open_repo(&path);
    let (matches, truncated) = search_tree(&repo, root, ".txt", 64, 5, 100).unwrap();
    assert!(matches.len() <= 5, "got {} matches", matches.len());
    assert!(truncated);
    drop(dir);
}
