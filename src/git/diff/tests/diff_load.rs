use crate::git::diff::{
    LineKind, StatusKind, load_commit_diff, load_commit_file_blob, load_commit_file_diff,
    load_commit_files, load_commit_log, load_file_diff, load_snapshot, load_workdir_file,
    parse_hunk_new_start,
};
use crate::test_util::{make_repo, open_repo, run_git};
use std::path::Path;

#[test]
fn root_commit_diff_lists_added_files() {
    let (dir, path) = make_repo();
    let fp = Path::new(&path).join("first.rs");
    std::fs::write(&fp, "fn main() {}\n").unwrap();
    run_git(&path, &["add", "."]);
    run_git(&path, &["commit", "-m", "init"]);

    let commits = load_commit_log(&open_repo(&path), 1).unwrap();
    let files = load_commit_files(&open_repo(&path), commits[0].oid).unwrap();
    let hunks = load_commit_diff(&open_repo(&path), commits[0].oid).unwrap();

    assert_eq!(files.len(), 1);
    assert_eq!(files[0].path, "first.rs");
    assert_eq!(files[0].index, StatusKind::Added);
    assert_eq!(files[0].worktree, StatusKind::Unmodified);
    assert!(
        hunks
            .iter()
            .flat_map(|h| &h.lines)
            .any(|line| line.kind == LineKind::Added && line.content.contains("fn main"))
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

    let hunks = load_file_diff(&open_repo(&path), "b.rs").unwrap();
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

    let hunks = load_file_diff(&open_repo(&path), "b.rs").unwrap();

    assert!(!hunks.is_empty());
    assert!(hunks[0].lines.iter().any(|l| l.kind == LineKind::Added));
    drop(dir);
}

#[test]
fn diff_returns_added_lines_for_staged_added_file() {
    let (dir, path) = make_repo();
    let fp = Path::new(&path).join("new.rs");
    std::fs::write(&fp, "fn main() {}\n").unwrap();
    run_git(&path, &["add", "new.rs"]);

    let hunks = load_file_diff(&open_repo(&path), "new.rs").unwrap();

    assert_eq!(hunks.len(), 1);
    assert_eq!(hunks[0].lines[0].kind, LineKind::Added);
    drop(dir);
}

#[test]
fn diff_returns_added_lines_for_untracked_file() {
    let (dir, path) = make_repo();
    let fp = Path::new(&path).join("new.rs");
    std::fs::write(&fp, "fn main() {}\n").unwrap();

    let snap = load_snapshot(&open_repo(&path)).unwrap();
    assert!(
        snap.files
            .iter()
            .any(|f| { f.path == "new.rs" && f.short_code() == "??" })
    );

    let hunks = load_file_diff(&open_repo(&path), "new.rs").unwrap();
    assert_eq!(hunks.len(), 1);
    assert_eq!(hunks[0].lines[0].kind, LineKind::Added);
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

    let hunks = load_file_diff(&open_repo(&path), "asset.bin").unwrap();

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

    let commits = load_commit_log(&open_repo(&path), 1).unwrap();
    let files = load_commit_files(&open_repo(&path), commits[0].oid).unwrap();

    assert_eq!(files.len(), 1);
    assert_eq!(files[0].path, "new.rs");
    assert_eq!(files[0].index, StatusKind::Renamed);
    assert_eq!(files[0].old_path.as_deref(), Some("old.rs"));
    assert_eq!(files[0].display_path(), "old.rs -> new.rs");
    drop(dir);
}

#[test]
fn parse_hunk_new_start_handles_standard_header() {
    assert_eq!(parse_hunk_new_start("@@ -1,3 +5,7 @@"), Some(5));
    assert_eq!(parse_hunk_new_start("@@ -10 +12 @@ ctx"), Some(12));
    assert_eq!(parse_hunk_new_start("@@ -0,0 +1,4 @@"), Some(1));
    assert_eq!(parse_hunk_new_start("diff src/foo.rs"), None);
    assert_eq!(parse_hunk_new_start("Binary file x changed"), None);
    assert_eq!(parse_hunk_new_start("@@"), None);
}

#[test]
fn load_workdir_file_reads_text_file() {
    let (dir, path) = make_repo();
    let fp = Path::new(&path).join("hello.txt");
    std::fs::write(&fp, "hi\nthere\n").unwrap();
    let content = load_workdir_file(&open_repo(&path), "hello.txt").unwrap();
    assert_eq!(content, "hi\nthere\n");
    drop(dir);
}

#[test]
fn load_workdir_file_rejects_binary() {
    let (dir, path) = make_repo();
    let fp = Path::new(&path).join("bin");
    std::fs::write(&fp, [0x00, 0xff, 0xfe]).unwrap();
    assert!(load_workdir_file(&open_repo(&path), "bin").is_err());
    drop(dir);
}

#[cfg(unix)]
#[test]
fn load_workdir_file_rejects_symlink_without_following() {
    let (dir, path) = make_repo();
    let target = Path::new(&path).join("target.txt");
    std::fs::write(&target, "secret\n").unwrap();
    std::os::unix::fs::symlink(&target, Path::new(&path).join("link.txt")).unwrap();

    let err = load_workdir_file(&open_repo(&path), "link.txt").unwrap_err();

    assert!(err.to_string().contains("symlinks are not followed"));
    drop(dir);
}

#[test]
fn load_workdir_file_rejects_paths_outside_the_worktree() {
    let (dir, path) = make_repo();

    let err = load_workdir_file(&open_repo(&path), "../../etc/passwd").unwrap_err();

    assert!(
        err.to_string().contains("plain relative path"),
        "unexpected error: {err}"
    );
    drop(dir);
}

#[test]
fn load_workdir_file_rejects_reading_the_git_directory() {
    let (dir, path) = make_repo();

    let err = load_workdir_file(&open_repo(&path), ".git/config").unwrap_err();

    assert!(
        err.to_string().contains("git directory"),
        "unexpected error: {err}"
    );
    drop(dir);
}

#[test]
fn load_commit_file_blob_reads_committed_text() {
    let (dir, path) = make_repo();
    let fp = Path::new(&path).join("a.txt");
    std::fs::write(&fp, "v1\n").unwrap();
    run_git(&path, &["add", "."]);
    run_git(&path, &["commit", "-m", "init"]);
    std::fs::write(&fp, "v2\n").unwrap();
    let commits = load_commit_log(&open_repo(&path), 1).unwrap();
    let content = load_commit_file_blob(
        &open_repo(&path),
        commits[0].oid,
        "a.txt",
        StatusKind::Modified,
    )
    .unwrap();
    assert_eq!(content, "v1\n");
    drop(dir);
}

#[test]
fn load_commit_file_blob_reads_deleted_file_from_parent() {
    let (dir, path) = make_repo();
    let fp = Path::new(&path).join("gone.txt");
    std::fs::write(&fp, "before delete\n").unwrap();
    run_git(&path, &["add", "."]);
    run_git(&path, &["commit", "-m", "add file"]);
    std::fs::remove_file(&fp).unwrap();
    run_git(&path, &["add", "."]);
    run_git(&path, &["commit", "-m", "delete file"]);

    let commits = load_commit_log(&open_repo(&path), 1).unwrap();
    let content = load_commit_file_blob(
        &open_repo(&path),
        commits[0].oid,
        "gone.txt",
        StatusKind::Deleted,
    )
    .unwrap();

    assert_eq!(content, "before delete\n");
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

    let commits = load_commit_log(&open_repo(&path), 1).unwrap();
    let hunks = load_commit_file_diff(&open_repo(&path), commits[0].oid, "new.rs").unwrap();

    assert!(!hunks.is_empty());
    assert!(
        hunks
            .iter()
            .flat_map(|h| &h.lines)
            .any(|l| l.kind == LineKind::Added && l.content.contains("println"))
    );
    drop(dir);
}
