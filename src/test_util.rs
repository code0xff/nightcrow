//! Shared helpers used only by the in-crate `#[cfg(test)]` modules.
//!
//! Several modules drive a real `git` binary against a `TempDir` to verify
//! repository discovery, commit walking, etc. Centralizing the setup here
//! keeps the helpers in one place instead of duplicating them per test
//! module.

#![cfg(test)]

use git2::Repository;
use std::process::Command;
use tempfile::TempDir;

pub fn open_repo(path: &str) -> Repository {
    Repository::discover(path).expect("discover test repo")
}

pub fn run_git(repo_path: &str, args: &[&str]) {
    let output = Command::new("git")
        .args(args)
        .current_dir(repo_path)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "git {} failed: {}",
        args.join(" "),
        String::from_utf8_lossy(&output.stderr)
    );
}

pub fn make_repo() -> (TempDir, String) {
    let dir = TempDir::new().unwrap();
    let path = dir.path().to_string_lossy().to_string();
    run_git(&path, &["init"]);
    run_git(&path, &["config", "user.email", "t@t.com"]);
    run_git(&path, &["config", "user.name", "T"]);
    (dir, path)
}
