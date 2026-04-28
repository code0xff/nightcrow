use anyhow::{Context, Result};
use git2::{Delta, Repository};
use std::cell::RefCell;

#[derive(Debug, Clone, PartialEq)]
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
    let mut files: Vec<ChangedFile> = Vec::new();

    let diff = match repo.diff_index_to_workdir(None, None) {
        Ok(d) => d,
        Err(_) => return Ok(RepoSnapshot { files }),
    };

    diff.foreach(
        &mut |delta, _| {
            let path = delta
                .new_file()
                .path()
                .or_else(|| delta.old_file().path())
                .map(|p| p.to_string_lossy().to_string())
                .unwrap_or_default();

            let status = match delta.status() {
                Delta::Added => ChangeStatus::Added,
                Delta::Deleted => ChangeStatus::Deleted,
                Delta::Renamed => ChangeStatus::Renamed,
                _ => ChangeStatus::Modified,
            };

            files.push(ChangedFile { path, status });
            true
        },
        None,
        None,
        None,
    )?;

    // Untracked files
    let mut opts = git2::StatusOptions::new();
    opts.include_untracked(true).recurse_untracked_dirs(false);
    if let Ok(statuses) = repo.statuses(Some(&mut opts)) {
        for entry in statuses.iter() {
            if entry.status().contains(git2::Status::WT_NEW) {
                let path = entry.path().unwrap_or("").to_string();
                if !path.is_empty() && !files.iter().any(|f| f.path == path) {
                    files.push(ChangedFile { path, status: ChangeStatus::Untracked });
                }
            }
        }
    }

    Ok(RepoSnapshot { files })
}

pub fn load_file_diff(repo_path: &str, file_path: &str) -> Result<Vec<DiffHunk>> {
    let repo = Repository::discover(repo_path).context("not a git repository")?;
    let mut diff_opts = git2::DiffOptions::new();
    diff_opts.pathspec(file_path);

    let diff = repo
        .diff_index_to_workdir(None, Some(&mut diff_opts))
        .context("failed to get diff")?;

    let hunks: RefCell<Vec<DiffHunk>> = RefCell::new(Vec::new());

    diff.foreach(
        &mut |_, _| true,
        None,
        Some(&mut |_, hunk| {
            let header = std::str::from_utf8(hunk.header())
                .unwrap_or("@@")
                .trim_end_matches('\n')
                .to_string();
            hunks.borrow_mut().push(DiffHunk { header, lines: Vec::new() });
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;
    use std::process::Command;
    use tempfile::TempDir;

    fn make_repo() -> (TempDir, String) {
        let dir = TempDir::new().unwrap();
        let p = dir.path().to_string_lossy().to_string();
        Command::new("git").args(["init"]).current_dir(&p).output().unwrap();
        Command::new("git")
            .args(["config", "user.email", "t@t.com"])
            .current_dir(&p)
            .output()
            .unwrap();
        Command::new("git")
            .args(["config", "user.name", "T"])
            .current_dir(&p)
            .output()
            .unwrap();
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
        Command::new("git").args(["add", "."]).current_dir(&path).output().unwrap();
        Command::new("git")
            .args(["commit", "-m", "init"])
            .current_dir(&path)
            .output()
            .unwrap();
        std::fs::write(&fp, "line1\nline2\n").unwrap();

        let snap = load_snapshot(&path).unwrap();
        assert!(snap.files.iter().any(|f| f.path.contains("a.txt")));
        drop(dir);
    }

    #[test]
    fn diff_returns_hunks_for_modified_file() {
        let (dir, path) = make_repo();
        let fp = Path::new(&path).join("b.rs");
        std::fs::write(&fp, "fn main() {}\n").unwrap();
        Command::new("git").args(["add", "."]).current_dir(&path).output().unwrap();
        Command::new("git")
            .args(["commit", "-m", "init"])
            .current_dir(&path)
            .output()
            .unwrap();
        std::fs::write(&fp, "fn main() {\n    println!(\"hi\");\n}\n").unwrap();

        let hunks = load_file_diff(&path, "b.rs").unwrap();
        assert!(!hunks.is_empty());
        assert!(hunks[0].lines.iter().any(|l| l.kind == LineKind::Added));
        drop(dir);
    }
}
