use anyhow::{Context, Result};
use git2::{Diff, DiffDelta, DiffOptions, Oid, Repository, Status, StatusEntry, StatusOptions};
use std::cell::RefCell;
use std::collections::BTreeMap;

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

#[derive(Debug, Clone)]
pub struct CommitEntry {
    pub oid: Oid,
    pub short_id: String,
    pub summary: String,
    pub author: String,
    pub time: i64,
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

pub fn load_commit_log(repo_path: &str, max_count: usize) -> Result<Vec<CommitEntry>> {
    let repo = Repository::discover(repo_path).context("not a git repository")?;
    let mut revwalk = repo.revwalk().context("failed to create revwalk")?;
    revwalk.push_head().context("failed to push HEAD")?;

    let mut entries = Vec::new();
    for oid_result in revwalk.take(max_count) {
        let oid = oid_result.context("revwalk error")?;
        let commit = repo.find_commit(oid).context("failed to find commit")?;
        let short_id = repo
            .find_object(oid, None)
            .and_then(|obj| obj.short_id())
            .map(|buf| buf.as_str().unwrap_or("").to_string())
            .unwrap_or_else(|_| format!("{:.7}", oid));
        let summary = commit.summary().unwrap_or("").to_string();
        let author = commit.author().name().unwrap_or("Unknown").to_string();
        let time = commit.time().seconds();
        entries.push(CommitEntry { oid, short_id, summary, author, time });
    }
    Ok(entries)
}

fn commit_diff<'repo>(
    repo: &'repo Repository,
    oid: Oid,
    pathspec: Option<&str>,
) -> Result<git2::Diff<'repo>> {
    let commit = repo.find_commit(oid).context("failed to find commit")?;
    let new_tree = commit.tree().context("failed to get commit tree")?;
    let old_tree = commit.parent(0).ok().and_then(|p| p.tree().ok());
    let mut diff_opts = diff_options(pathspec);
    let mut diff = repo
        .diff_tree_to_tree(old_tree.as_ref(), Some(&new_tree), Some(&mut diff_opts))
        .context("failed to get commit diff")?;
    diff.find_similar(None).context("failed to detect renames")?;
    Ok(diff)
}

pub fn load_commit_files(repo_path: &str, oid: Oid) -> Result<Vec<ChangedFile>> {
    let repo = Repository::discover(repo_path).context("not a git repository")?;
    let diff = commit_diff(&repo, oid, None)?;
    let mut files = Vec::new();
    for delta in diff.deltas() {
        let status = match delta.status() {
            git2::Delta::Added => ChangeStatus::Added,
            git2::Delta::Deleted => ChangeStatus::Deleted,
            git2::Delta::Renamed => ChangeStatus::Renamed,
            _ => ChangeStatus::Modified,
        };
        let path = path_from_delta(delta).unwrap_or_else(|| "unknown".to_string());
        files.push(ChangedFile { path, status });
    }
    Ok(files)
}

pub fn load_commit_file_diff(repo_path: &str, oid: Oid, file_path: &str) -> Result<Vec<DiffHunk>> {
    let repo = Repository::discover(repo_path).context("not a git repository")?;
    let diff = commit_diff(&repo, oid, Some(file_path))?;
    collect_commit_diff_hunks(&diff)
}

pub fn load_commit_diff(repo_path: &str, oid: Oid) -> Result<Vec<DiffHunk>> {
    let repo = Repository::discover(repo_path).context("not a git repository")?;
    let diff = commit_diff(&repo, oid, None)?;
    collect_commit_diff_hunks(&diff)
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

fn collect_commit_diff_hunks(diff: &Diff<'_>) -> Result<Vec<DiffHunk>> {
    let hunks: RefCell<Vec<DiffHunk>> = RefCell::new(Vec::new());

    diff.foreach(
        &mut |delta, _| {
            let path = path_from_delta(delta).unwrap_or_else(|| "unknown".to_string());
            hunks.borrow_mut().push(DiffHunk {
                header: format!("diff {path}"),
                lines: Vec::new(),
            });
            true
        },
        Some(&mut |delta, _| {
            let path = path_from_delta(delta).unwrap_or_else(|| "unknown".to_string());
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

fn binary_diff_hunk(file_path: &str) -> DiffHunk {
    DiffHunk {
        header: format!("Binary file {file_path} changed"),
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

    #[test]
    fn commit_files_detects_renamed_file() {
        let (dir, path) = make_repo();
        let old_path = Path::new(&path).join("old.rs");
        std::fs::write(&old_path, "fn main() {}\n").unwrap();
        run_git(&path, &["add", "."]);
        run_git(&path, &["commit", "-m", "init"]);
        run_git(&path, &["mv", "old.rs", "new.rs"]);
        run_git(&path, &["commit", "-m", "rename"]);

        let commits = load_commit_log(&path, 1).unwrap();
        let files = load_commit_files(&path, commits[0].oid).unwrap();

        assert_eq!(files.len(), 1);
        assert_eq!(files[0].path, "new.rs");
        assert_eq!(files[0].status, ChangeStatus::Renamed);
        drop(dir);
    }

    #[test]
    fn commit_file_diff_returns_renamed_file_diff() {
        let (dir, path) = make_repo();
        let old_path = Path::new(&path).join("old.rs");
        std::fs::write(&old_path, "fn main() {}\n").unwrap();
        run_git(&path, &["add", "."]);
        run_git(&path, &["commit", "-m", "init"]);
        run_git(&path, &["mv", "old.rs", "new.rs"]);
        std::fs::write(
            Path::new(&path).join("new.rs"),
            "fn main() {\n    println!(\"hi\");\n}\n",
        )
        .unwrap();
        run_git(&path, &["add", "."]);
        run_git(&path, &["commit", "-m", "rename and edit"]);

        let commits = load_commit_log(&path, 1).unwrap();
        let hunks = load_commit_file_diff(&path, commits[0].oid, "new.rs").unwrap();

        assert!(!hunks.is_empty());
        assert!(
            hunks
                .iter()
                .flat_map(|h| &h.lines)
                .any(|l| l.kind == LineKind::Added && l.content.contains("println"))
        );
        drop(dir);
    }
}
