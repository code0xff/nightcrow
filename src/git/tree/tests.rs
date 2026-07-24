use super::*;
use crate::test_util::{make_repo, open_repo, run_git};
use std::path::Path as StdPath;

fn names(entries: &[TreeEntry]) -> Vec<&str> {
    entries.iter().map(|e| e.name.as_str()).collect()
}

#[test]
fn read_children_sorts_dirs_first_then_alpha() {
    let (dir, path) = make_repo();
    let root = StdPath::new(&path);
    std::fs::create_dir(root.join("zeta_dir")).unwrap();
    std::fs::create_dir(root.join("alpha_dir")).unwrap();
    std::fs::write(root.join("b_file.txt"), "x").unwrap();
    std::fs::write(root.join("a_file.txt"), "x").unwrap();

    let workdir = open_repo(&path);
    let entries = read_children(&workdir, root, "", true).unwrap();

    assert_eq!(
        names(&entries),
        vec!["alpha_dir", "zeta_dir", "a_file.txt", "b_file.txt"]
    );
    assert!(entries[0].is_dir);
    assert!(entries[1].is_dir);
    assert!(!entries[2].is_dir);
    drop(dir);
}

#[test]
fn read_children_reads_nested_dir_lazily() {
    let (dir, path) = make_repo();
    let root = StdPath::new(&path);
    std::fs::create_dir_all(root.join("src").join("ui")).unwrap();
    std::fs::write(root.join("src").join("main.rs"), "fn main() {}").unwrap();

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

    // `.git` exists on disk (make_repo runs `git init`) but must never be
    // listed.
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
    // Commit the gitignore so libgit2 picks it up reliably.
    run_git(&path, &["add", ".gitignore"]);
    run_git(&path, &["commit", "-m", "add gitignore"]);

    let repo = open_repo(&path);
    let filtered = read_children(&repo, root, "", true).unwrap();
    assert!(!names(&filtered).contains(&"ignored.log"));
    assert!(!names(&filtered).contains(&"build"));
    assert!(names(&filtered).contains(&"kept.rs"));

    // With the toggle off, ignored paths reappear.
    let unfiltered = read_children(&repo, root, "", false).unwrap();
    assert!(names(&unfiltered).contains(&"ignored.log"));
    assert!(names(&unfiltered).contains(&"build"));
    drop(dir);
}

#[cfg(unix)]
#[test]
fn read_children_refuses_to_list_the_git_directory() {
    // Skipping `.git` from child listings is not enough: asking for it as
    // the directory itself enumerated refs, hooks, and object layout.
    let (dir, path) = make_repo();
    let root = StdPath::new(&path);
    let repo = open_repo(&path);

    for asked in [".git", ".git/refs", ".GIT", "src/../.git"] {
        let err = read_children(&repo, root, asked, false).unwrap_err();
        assert!(
            !err.to_string().contains("failed to read directory"),
            "{asked:?} must be refused by validation, not by chance: {err}"
        );
    }
    drop(dir);
}

#[test]
fn read_children_refuses_traversal_that_stays_inside_the_worktree() {
    // Containment alone accepts this — it resolves back inside the repo.
    let (dir, path) = make_repo();
    let root = StdPath::new(&path);
    std::fs::create_dir(root.join("src")).unwrap();
    let repo = open_repo(&path);

    let err = read_children(&repo, root, "src/..", false).unwrap_err();

    assert!(
        err.to_string().contains("plain relative path"),
        "unexpected error: {err}"
    );
    drop(dir);
}

#[test]
fn read_children_refuses_to_descend_a_symlinked_directory() {
    let (dir, path) = make_repo();
    let root = StdPath::new(&path);
    std::fs::create_dir(root.join("real_dir")).unwrap();
    std::fs::write(root.join("real_dir").join("secret.txt"), "x").unwrap();
    std::os::unix::fs::symlink(root.join("real_dir"), root.join("link_dir")).unwrap();

    let repo = open_repo(&path);
    // Expanding the symlink path directly (as a stale session or swapped
    // cache entry could ask to) must be rejected, not followed.
    let err = read_children(&repo, root, "link_dir", false).unwrap_err();
    assert!(
        err.to_string().contains("symlinks are not followed"),
        "unexpected error: {err}"
    );
    // The real directory still reads normally.
    assert!(read_children(&repo, root, "real_dir", false).is_ok());
    drop(dir);
}

#[cfg(unix)]
#[test]
fn read_children_rejects_escape_through_symlinked_parent() {
    let (dir, path) = make_repo();
    let root = StdPath::new(&path);
    // An external tree outside the repo, reachable via a symlinked parent.
    let outside = tempfile::TempDir::new().unwrap();
    std::fs::create_dir(outside.path().join("sub")).unwrap();
    std::fs::write(outside.path().join("sub").join("secret.txt"), "x").unwrap();
    std::os::unix::fs::symlink(outside.path(), root.join("link")).unwrap();

    let repo = open_repo(&path);
    // `link` is a symlink (intermediate component); `link/sub` must be
    // rejected at the link, before the path is ever resolved outside.
    let err = read_children(&repo, root, "link/sub", false).unwrap_err();
    assert!(
        err.to_string().contains("symlinks are not followed"),
        "unexpected error: {err}"
    );
    drop(dir);
}

fn paths(matches: &[TreeMatch]) -> Vec<&str> {
    matches.iter().map(|m| m.path.as_str()).collect()
}

#[test]
fn search_tree_finds_nested_matches_by_basename() {
    let (dir, path) = make_repo();
    let root = StdPath::new(&path);
    std::fs::create_dir_all(root.join("src").join("ui")).unwrap();
    std::fs::write(root.join("src").join("main.rs"), "x").unwrap();
    std::fs::write(root.join("src").join("ui").join("tree_view.rs"), "x").unwrap();
    std::fs::write(root.join("README.md"), "x").unwrap();

    let repo = open_repo(&path);
    // Case-insensitive substring on the basename, matched recursively; sorted
    // by full path.
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
    std::fs::write(root.join("target").join("build.rs"), "x").unwrap();
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
    std::fs::create_dir_all(root.join("a").join("b")).unwrap();
    std::fs::write(root.join("a").join("shallow.txt"), "x").unwrap();
    std::fs::write(root.join("a").join("b").join("deep.txt"), "x").unwrap();

    let repo = open_repo(&path);
    // depth 0 = root's children ("a"); depth 1 = "a"'s children ("b",
    // "shallow.txt"). With max_depth 1, "b"'s children are never read.
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
    assert_eq!(matches.len(), 3);
    assert!(truncated);
    // Deterministic prefix: sorted then truncated.
    assert_eq!(paths(&matches), vec!["hit_0.txt", "hit_1.txt", "hit_2.txt"]);
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
    // A tight visit budget cuts the walk short before every entry is seen, so
    // the result is flagged incomplete even though it is under the result cap.
    let (matches, truncated) = search_tree(&repo, root, ".txt", 64, 5, 100).unwrap();
    assert!(matches.len() <= 5, "got {} matches", matches.len());
    assert!(truncated);
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
        .find(|e| e.name == "link_dir")
        .expect("symlink should be listed");
    // A symlinked directory must report `is_dir == false` so the navigator
    // treats it as a leaf and never follows it (cycle guard).
    assert!(!link.is_dir);
    let real = entries.iter().find(|e| e.name == "real_dir").unwrap();
    assert!(real.is_dir);
    drop(dir);
}