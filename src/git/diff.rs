use anyhow::{Context, Result};
use git2::{Diff, DiffDelta, DiffOptions, Repository, Status, StatusEntry, StatusOptions};
use std::cell::RefCell;
use std::collections::BTreeMap;
use std::path::Path;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChangeStatus {
    Added,
    Modified,
    Deleted,
    Renamed,
    Untracked,
}

impl ChangeStatus {
    pub fn symbol(&self) -> &'static str {
        match self {
            Self::Added => "A",
            Self::Modified => "M",
            Self::Deleted => "D",
            Self::Renamed => "R",
            Self::Untracked => "?",
        }
    }
}

#[derive(Debug, Clone)]
pub struct ChangedFile {
    pub path: String,
    pub status: ChangeStatus,
}

#[derive(Debug, Clone, PartialEq)]
pub enum LineKind {
    Added,
    Removed,
    Context,
}

#[derive(Debug, Clone)]
pub struct DiffLine {
    pub kind: LineKind,
    pub content: String,
}

#[derive(Debug, Clone)]
pub struct DiffHunk {
    pub header: String,
    pub lines: Vec<DiffLine>,
}

#[derive(Debug, Clone)]
pub struct RepoSnapshot {
    pub files: Vec<ChangedFile>,
}

pub fn load_snapshot(repo_path: &str) -> Result<RepoSnapshot> {
    let repo = Repository::discover(repo_path).context("not a git repository")?;

    let mut opts = StatusOptions::new();
    opts.include_untracked(true)
        .recurse_untracked_dirs(true)
        .renames_head_to_index(true)
        .renames_index_to_workdir(true);

    let statuses = repo
        .statuses(Some(&mut opts))
        .context("failed to get repository status")?;

    let mut files = BTreeMap::new();
    for entry in statuses.iter() {
        let Some(status) = change_status_from_git_status(entry.status()) else {
            continue;
        };
        if let Some(path) = path_from_status_entry(&entry) && !path.is_empty() {
            files.entry(path).or_insert(status);
        }
    }

    let files = files
        .into_iter()
        .map(|(path, status)| ChangedFile { path, status })
        .collect();

    Ok(RepoSnapshot { files })
}

pub fn load_file_diff(repo_path: &str, file_path: &str) -> Result<Vec<DiffHunk>> {
    let repo = Repository::discover(repo_path).context("not a git repository")?;
    let head_tree = repo.head().ok().and_then(|head| head.peel_to_tree().ok());
    let mut diff_opts = diff_options(Some(file_path));

    let mut diff = repo
        .diff_tree_to_workdir_with_index(head_tree.as_ref(), Some(&mut diff_opts))
        .context("failed to get diff")?;

    diff.find_similar(None)
        .context("failed to detect renamed files")?;

    collect_diff_hunks(&diff, file_path)
}

fn diff_options(pathspec: Option<&str>) -> DiffOptions {
    let mut opts = DiffOptions::new();
    opts.include_untracked(true)
        .recurse_untracked_dirs(true)
        .show_untracked_content(true)
        .show_binary(true);
    if let Some(pathspec) = pathspec {
        opts.pathspec(pathspec).disable_pathspec_match(true);
    }
    opts
}

fn collect_diff_hunks(diff: &Diff<'_>, fallback_path: &str) -> Result<Vec<DiffHunk>> {
    let hunks: RefCell<Vec<DiffHunk>> = RefCell::new(Vec::new());

    diff.foreach(
        &mut |_, _| true,
        Some(&mut |delta, _| {
            let path = path_from_delta(delta).unwrap_or_else(|| fallback_path.to_string());
            hunks.borrow_mut().push(binary_diff_hunk(&path));
            true
        }),
        Some(&mut |_, hunk| {
            let header = std::str::from_utf8(hunk.header())
                .unwrap_or("@@")
                .trim_end_matches('\n')
                .to_string();
            hunks.borrow_mut().push(DiffHunk {
                header,
                lines: Vec::new(),
            });
            true
        }),
        Some(&mut |_, _, line| {
            let content = std::str::from_utf8(line.content())
                .unwrap_or("")
                .trim_end_matches('\n')
                .to_string();
            let kind = match line.origin() {
                '+' => LineKind::Added,
                '-' => LineKind::Removed,
                '\\' => return true,
                _ => LineKind::Context,
            };
            if let Some(h) = hunks.borrow_mut().last_mut() {
                h.lines.push(DiffLine { kind, content });
            }
            true
        }),
    )?;

    Ok(hunks.into_inner())
}

fn change_status_from_git_status(status: Status) -> Option<ChangeStatus> {
    if status.contains(Status::WT_NEW) && !status.contains(Status::INDEX_NEW) {
        Some(ChangeStatus::Untracked)
    } else if status.intersects(Status::INDEX_RENAMED | Status::WT_RENAMED) {
        Some(ChangeStatus::Renamed)
    } else if status.intersects(Status::INDEX_DELETED | Status::WT_DELETED) {
        Some(ChangeStatus::Deleted)
    } else if status.contains(Status::INDEX_NEW) {
        Some(ChangeStatus::Added)
    } else if status.intersects(
        Status::INDEX_MODIFIED
            | Status::WT_MODIFIED
            | Status::INDEX_TYPECHANGE
            | Status::WT_TYPECHANGE
            | Status::WT_UNREADABLE
            | Status::CONFLICTED,
    ) {
        Some(ChangeStatus::Modified)
    } else {
        None
    }
}

fn path_from_status_entry(entry: &StatusEntry<'_>) -> Option<String> {
    entry
        .index_to_workdir()
        .and_then(path_from_delta)
        .or_else(|| entry.head_to_index().and_then(path_from_delta))
        .or_else(|| entry.path().map(str::to_string))
}

fn path_from_delta(delta: DiffDelta<'_>) -> Option<String> {
    delta
        .new_file()
        .path()
        .or_else(|| delta.old_file().path())
        .map(|p| p.to_string_lossy().to_string())
}

fn display_path(file_path: &str) -> String {
    Path::new(file_path).display().to_string()
}

fn binary_diff_hunk(file_path: &str) -> DiffHunk {
    DiffHunk {
        header: format!("Binary file {} changed", display_path(file_path)),
        lines: vec![DiffLine {
            kind: LineKind::Context,
            content: "Binary files differ".to_string(),
        }],
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;
    use std::process::Command;
    use tempfile::TempDir;

    fn run_git(repo_path: &str, args: &[&str]) {
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

    fn make_repo() -> (TempDir, String) {
        let dir = TempDir::new().unwrap();
        let p = dir.path().to_string_lossy().to_string();
        run_git(&p, &["init"]);
        run_git(&p, &["config", "user.email", "t@t.com"]);
        run_git(&p, &["config", "user.name", "T"]);
        (dir, p)
    }

    #[test]
    fn snapshot_empty_repo_does_not_panic() {
        let (dir, path) = make_repo();
        let _ = load_snapshot(&path);
        drop(dir);
    }

    #[test]
    fn snapshot_detects_modified_file() {
        let (dir, path) = make_repo();
        let fp = Path::new(&path).join("a.txt");
        std::fs::write(&fp, "line1\n").unwrap();
        run_git(&path, &["add", "."]);
        run_git(&path, &["commit", "-m", "init"]);
        std::fs::write(&fp, "line1\nline2\n").unwrap();

        let snap = load_snapshot(&path).unwrap();
        assert!(snap.files.iter().any(|f| f.path.contains("a.txt")));
        drop(dir);
    }

    #[test]
    fn snapshot_detects_staged_modified_file() {
        let (dir, path) = make_repo();
        let fp = Path::new(&path).join("a.txt");
        std::fs::write(&fp, "line1\n").unwrap();
        run_git(&path, &["add", "."]);
        run_git(&path, &["commit", "-m", "init"]);
        std::fs::write(&fp, "line1\nline2\n").unwrap();
        run_git(&path, &["add", "a.txt"]);

        let snap = load_snapshot(&path).unwrap();

        assert!(
            snap.files
                .iter()
                .any(|f| f.path == "a.txt" && matches!(f.status, ChangeStatus::Modified))
        );
        drop(dir);
    }

    #[test]
    fn diff_returns_hunks_for_modified_file() {
        let (dir, path) = make_repo();
        let fp = Path::new(&path).join("b.rs");
        std::fs::write(&fp, "fn main() {}\n").unwrap();
        run_git(&path, &["add", "."]);
        run_git(&path, &["commit", "-m", "init"]);
        std::fs::write(&fp, "fn main() {\n    println!(\"hi\");\n}\n").unwrap();

        let hunks = load_file_diff(&path, "b.rs").unwrap();
        assert!(!hunks.is_empty());
        assert!(hunks[0].lines.iter().any(|l| l.kind == LineKind::Added));
        drop(dir);
    }

    #[test]
    fn diff_returns_hunks_for_staged_modified_file() {
        let (dir, path) = make_repo();
        let fp = Path::new(&path).join("b.rs");
        std::fs::write(&fp, "fn main() {}\n").unwrap();
        run_git(&path, &["add", "."]);
        run_git(&path, &["commit", "-m", "init"]);
        std::fs::write(&fp, "fn main() {\n    println!(\"hi\");\n}\n").unwrap();
        run_git(&path, &["add", "b.rs"]);

        let hunks = load_file_diff(&path, "b.rs").unwrap();

        assert!(!hunks.is_empty());
        assert!(hunks[0].lines.iter().any(|l| l.kind == LineKind::Added));
        drop(dir);
    }

    #[test]
    fn snapshot_detects_staged_added_file() {
        let (dir, path) = make_repo();
        let fp = Path::new(&path).join("new.rs");
        std::fs::write(&fp, "fn main() {}\n").unwrap();
        run_git(&path, &["add", "new.rs"]);

        let snap = load_snapshot(&path).unwrap();

        assert!(
            snap.files
                .iter()
                .any(|f| f.path == "new.rs" && matches!(f.status, ChangeStatus::Added))
        );
        drop(dir);
    }

    #[test]
    fn diff_returns_added_lines_for_staged_added_file() {
        let (dir, path) = make_repo();
        let fp = Path::new(&path).join("new.rs");
        std::fs::write(&fp, "fn main() {}\n").unwrap();
        run_git(&path, &["add", "new.rs"]);

        let hunks = load_file_diff(&path, "new.rs").unwrap();

        assert_eq!(hunks.len(), 1);
        assert_eq!(hunks[0].lines[0].kind, LineKind::Added);
        drop(dir);
    }

    #[test]
    fn diff_returns_added_lines_for_untracked_file() {
        let (dir, path) = make_repo();
        let fp = Path::new(&path).join("new.rs");
        std::fs::write(&fp, "fn main() {}\n").unwrap();

        let snap = load_snapshot(&path).unwrap();
        assert!(
            snap.files
                .iter()
                .any(|f| { f.path == "new.rs" && matches!(f.status, ChangeStatus::Untracked) })
        );

        let hunks = load_file_diff(&path, "new.rs").unwrap();
        assert_eq!(hunks.len(), 1);
        assert_eq!(hunks[0].lines[0].kind, LineKind::Added);
        drop(dir);
    }

    #[test]
    fn snapshot_recurses_untracked_directories() {
        let (dir, path) = make_repo();
        let nested = Path::new(&path).join("src").join("new.rs");
        std::fs::create_dir_all(nested.parent().unwrap()).unwrap();
        std::fs::write(&nested, "fn main() {}\n").unwrap();

        let snap = load_snapshot(&path).unwrap();

        assert!(snap.files.iter().any(|f| f.path == "src/new.rs"));
        drop(dir);
    }

    #[test]
    fn diff_returns_placeholder_for_binary_file() {
        let (dir, path) = make_repo();
        let fp = Path::new(&path).join("asset.bin");
        std::fs::write(&fp, [0, 1, 2]).unwrap();
        run_git(&path, &["add", "."]);
        run_git(&path, &["commit", "-m", "init"]);
        std::fs::write(&fp, [0, 1, 3]).unwrap();

        let hunks = load_file_diff(&path, "asset.bin").unwrap();

        assert_eq!(hunks.len(), 1);
        assert!(hunks[0].header.contains("Binary file"));
        drop(dir);
    }
}
