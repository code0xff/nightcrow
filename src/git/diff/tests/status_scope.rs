//! What a status covers, and what it leaves out.
//!
//! The list's *scope* rather than its contents: which paths git is asked about at
//! all. Everything downstream — the ceiling on one payload, the cost of a read,
//! whether a filesystem event is worth reacting to — is sized by the answer.

use crate::git::diff::load_snapshot;
use crate::test_util::{make_repo, open_repo};
use std::path::Path;

#[test]
fn ignored_paths_never_reach_the_status() {
    // What keeps the status list a human-sized thing: a repository with
    // `node_modules` or `target` in it has tens of thousands of files that git
    // is told to ignore, and none of them are changes anyone asked about. The
    // ceiling the viewer puts on one payload is therefore reached by real change
    // volume, not by build output — worth pinning, because turning
    // `include_ignored` on would quietly make every JS or Rust checkout
    // unreadable.
    let (dir, path) = make_repo();
    std::fs::write(Path::new(&path).join(".gitignore"), "ignored/\n*.log\n").unwrap();
    std::fs::create_dir_all(Path::new(&path).join("ignored")).unwrap();
    std::fs::write(Path::new(&path).join("ignored").join("a.js"), "x").unwrap();
    std::fs::write(Path::new(&path).join("build.log"), "x").unwrap();
    std::fs::write(Path::new(&path).join("kept.rs"), "fn main() {}").unwrap();

    let snapshot = load_snapshot(&open_repo(&path)).expect("status");

    let paths: Vec<&str> = snapshot.files.iter().map(|f| f.path.as_str()).collect();
    assert!(
        paths.contains(&"kept.rs"),
        "untracked-but-not-ignored: {paths:?}"
    );
    assert!(paths.contains(&".gitignore"));
    assert!(
        !paths.iter().any(|p| p.starts_with("ignored/")),
        "an ignored directory must not appear: {paths:?}"
    );
    assert!(
        !paths.contains(&"build.log"),
        "an ignored pattern must not appear: {paths:?}"
    );
    drop(dir);
}
