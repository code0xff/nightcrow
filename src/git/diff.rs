use anyhow::{Context, Result, bail};
use git2::{Delta, Repository};
use std::cell::RefCell;
use std::path::Path;

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

    let diff = repo
        .diff_index_to_workdir(None, None)
        .context("failed to get workdir diff")?;

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
    opts.include_untracked(true).recurse_untracked_dirs(true);
    let statuses = repo
        .statuses(Some(&mut opts))
        .context("failed to get repository status")?;
    for entry in statuses.iter() {
        if entry.status().contains(git2::Status::WT_NEW) {
            let path = entry.path().unwrap_or("").to_string();
            if !path.is_empty() && !files.iter().any(|f| f.path == path) {
                files.push(ChangedFile {
                    path,
                    status: ChangeStatus::Untracked,
                });
            }
        }
    }

    files.sort_by(|a, b| a.path.cmp(&b.path));

    Ok(RepoSnapshot { files })
}

pub fn load_file_diff(repo_path: &str, file_path: &str) -> Result<Vec<DiffHunk>> {
    let repo = Repository::discover(repo_path).context("not a git repository")?;

    if is_untracked(&repo, file_path)? {
        return load_untracked_file_diff(&repo, file_path);
    }

    let mut diff_opts = git2::DiffOptions::new();
    diff_opts.pathspec(file_path).show_binary(true);

    let diff = repo
        .diff_index_to_workdir(None, Some(&mut diff_opts))
        .context("failed to get diff")?;

    let hunks: RefCell<Vec<DiffHunk>> = RefCell::new(Vec::new());

    diff.foreach(
        &mut |_, _| true,
        Some(&mut |delta, _| {
            let path = delta
                .new_file()
                .path()
                .or_else(|| delta.old_file().path())
                .map(|p| p.to_string_lossy().to_string())
                .unwrap_or_else(|| file_path.to_string());
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

fn is_untracked(repo: &Repository, file_path: &str) -> Result<bool> {
    let mut opts = git2::StatusOptions::new();
    opts.include_untracked(true)
        .recurse_untracked_dirs(true)
        .pathspec(file_path);

    let statuses = repo.statuses(Some(&mut opts))?;
    Ok(statuses
        .iter()
        .any(|entry| entry.status().contains(git2::Status::WT_NEW)))
}

fn load_untracked_file_diff(repo: &Repository, file_path: &str) -> Result<Vec<DiffHunk>> {
    let workdir = repo.workdir().context("repository has no workdir")?;
    let full_path = workdir.join(file_path);
    let canonical_workdir = workdir
        .canonicalize()
        .context("failed to resolve workdir")?;
    let canonical_file = full_path
        .canonicalize()
        .context("failed to resolve untracked file")?;

    if !canonical_file.starts_with(&canonical_workdir) {
        bail!("untracked path escapes repository workdir");
    }

    let bytes = std::fs::read(&canonical_file)
        .with_context(|| format!("failed to read {}", display_path(file_path)))?;
    let content = match String::from_utf8(bytes) {
        Ok(content) => content,
        Err(_) => return Ok(vec![binary_diff_hunk(file_path)]),
    };
    let line_count = content.lines().count();
    let lines = content
        .lines()
        .map(|line| DiffLine {
            kind: LineKind::Added,
            content: line.to_string(),
        })
        .collect();

    Ok(vec![DiffHunk {
        header: format!("@@ -0,0 +1,{line_count} @@"),
        lines,
    }])
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
