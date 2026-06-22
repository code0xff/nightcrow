//! Lazy, read-only directory listing for the file-tree navigator.
//!
//! Each call reads exactly one directory level (`std::fs::read_dir`); the
//! caller decides when to descend, so an unexpanded subtree is never walked.
//! Listing is filtered against `.gitignore` (via libgit2) and repository
//! metadata, and symlinks are reported as non-directories so the navigator
//! never follows one — this is what keeps the tree free of cycles without a
//! visited-set.

use anyhow::{Context, Result};
use git2::Repository;
use std::path::Path;

/// One immediate child of a directory. `is_dir` is taken from the entry's own
/// file type (symlinks resolve to `false`), so a symlinked directory shows up
/// as a non-expandable row and is never descended into.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TreeEntry {
    pub name: String,
    pub is_dir: bool,
}

/// Read the immediate children of `rel_dir` (a repo-relative path; `""` is the
/// workdir root). Entries are filtered and returned sorted with directories
/// first, then case-sensitive alphabetical by name.
///
/// Filtering rules:
/// - `.git` is skipped at every level (repository metadata / object storage).
/// - When `respect_gitignore` is set, ignored paths are dropped via
///   `Repository::is_path_ignored`.
/// - Non-UTF-8 names are skipped: the file-view loader keys on `&str` paths and
///   cannot losslessly address them.
/// - Individual entries whose metadata cannot be read are skipped rather than
///   failing the whole listing.
pub fn read_children(
    repo: &Repository,
    workdir: &Path,
    rel_dir: &str,
    respect_gitignore: bool,
) -> Result<Vec<TreeEntry>> {
    let abs_dir = if rel_dir.is_empty() {
        workdir.to_path_buf()
    } else {
        workdir.join(rel_dir)
    };
    let read = std::fs::read_dir(&abs_dir)
        .with_context(|| format!("failed to read directory {}", abs_dir.display()))?;

    let mut out = Vec::new();
    for entry in read {
        let Ok(entry) = entry else { continue };
        // Non-UTF-8 names cannot be addressed by the `&str`-keyed file-view
        // loader, so they are dropped from the tree entirely.
        let Ok(name) = entry.file_name().into_string() else {
            continue;
        };
        // `.git` is repository metadata, not browsable project content. git
        // does not list it in `.gitignore`, so it must be skipped explicitly.
        if name == ".git" {
            continue;
        }
        // `file_type()` does NOT follow symlinks, so a symlinked directory
        // reports `is_dir() == false` and becomes a non-expandable row — the
        // navigator therefore never descends a link and cannot cycle.
        let Ok(file_type) = entry.file_type() else {
            continue;
        };
        let is_dir = file_type.is_dir();

        let rel_path = if rel_dir.is_empty() {
            name.clone()
        } else {
            format!("{rel_dir}/{name}")
        };
        if respect_gitignore
            && repo
                .is_path_ignored(Path::new(&rel_path))
                .unwrap_or(false)
        {
            continue;
        }
        out.push(TreeEntry { name, is_dir });
    }

    // Directories first (true sorts after false, so compare reversed), then
    // case-sensitive alphabetical — stable, predictable ordering for keyboard
    // navigation.
    out.sort_by(|a, b| b.is_dir.cmp(&a.is_dir).then_with(|| a.name.cmp(&b.name)));
    Ok(out)
}

#[cfg(test)]
mod tests {
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
}
