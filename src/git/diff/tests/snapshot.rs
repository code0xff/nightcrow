use crate::git::diff::{ChangedFile, RepoSnapshot, StatusKind, load_file_diff, load_snapshot};
use crate::test_util::{make_repo, open_repo, run_git};
use std::borrow::Cow;
use std::path::Path;

#[test]
fn snapshot_empty_repo_does_not_panic() {
    let (dir, path) = make_repo();
    let _ = load_snapshot(&open_repo(&path));
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

    let snap = load_snapshot(&open_repo(&path)).unwrap();
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

    let snap = load_snapshot(&open_repo(&path)).unwrap();

    assert!(snap.files.iter().any(|f| f.path == "a.txt"
        && f.index == StatusKind::Modified
        && f.worktree == StatusKind::Unmodified
        && f.short_code() == "M "));
    drop(dir);
}

#[test]
fn snapshot_detects_staged_added_file() {
    let (dir, path) = make_repo();
    let fp = Path::new(&path).join("new.rs");
    std::fs::write(&fp, "fn main() {}\n").unwrap();
    run_git(&path, &["add", "new.rs"]);

    let snap = load_snapshot(&open_repo(&path)).unwrap();

    assert!(
        snap.files
            .iter()
            .any(|f| f.path == "new.rs" && f.index == StatusKind::Added && f.short_code() == "A ")
    );
    drop(dir);
}

#[test]
fn snapshot_recurses_untracked_directories() {
    let (dir, path) = make_repo();
    let nested = Path::new(&path).join("src").join("new.rs");
    std::fs::create_dir_all(nested.parent().unwrap()).unwrap();
    std::fs::write(&nested, "fn main() {}\n").unwrap();

    let snap = load_snapshot(&open_repo(&path)).unwrap();

    assert!(snap.files.iter().any(|f| f.path == "src/new.rs"));
    drop(dir);
}

// --- Workstream 1: XY status model unit tests (no git needed) ---

#[test]
fn short_code_renders_each_xy_combination() {
    let mk = |index, worktree| {
        ChangedFile::from_status_columns("p".into(), None, index, worktree).short_code()
    };
    assert_eq!(mk(StatusKind::Unmodified, StatusKind::Modified), " M");
    assert_eq!(mk(StatusKind::Modified, StatusKind::Unmodified), "M ");
    assert_eq!(mk(StatusKind::Modified, StatusKind::Modified), "MM");
    assert_eq!(mk(StatusKind::Added, StatusKind::Unmodified), "A ");
    assert_eq!(mk(StatusKind::Renamed, StatusKind::Unmodified), "R ");
    assert_eq!(mk(StatusKind::TypeChanged, StatusKind::Unmodified), "T ");
    // Untracked and conflicted collapse to git's two-column sentinels
    // regardless of which column carries the bit.
    assert_eq!(mk(StatusKind::Untracked, StatusKind::Untracked), "??");
    assert_eq!(mk(StatusKind::Unmerged, StatusKind::Unmerged), "UU");
}

#[test]
fn display_path_borrows_for_non_rename_and_formats_rename() {
    let plain = ChangedFile::from_status_columns(
        "src/a.rs".into(),
        None,
        StatusKind::Modified,
        StatusKind::Unmodified,
    );
    assert!(matches!(plain.display_path(), Cow::Borrowed(_)));
    assert_eq!(plain.display_path(), "src/a.rs");

    let renamed = ChangedFile::from_status_columns(
        "new.rs".into(),
        Some("old.rs".into()),
        StatusKind::Renamed,
        StatusKind::Unmodified,
    );
    assert!(matches!(renamed.display_path(), Cow::Owned(_)));
    assert_eq!(renamed.display_path(), "old.rs -> new.rs");
    // Search text matches either side of a rename.
    assert!(renamed.search_lower.contains("old.rs"));
    assert!(renamed.search_lower.contains("new.rs"));
}

#[test]
fn most_severe_picks_higher_severity_column() {
    // Deleted outranks modified regardless of column.
    let f = ChangedFile::from_status_columns(
        "p".into(),
        None,
        StatusKind::Modified,
        StatusKind::Deleted,
    );
    assert_eq!(f.most_severe(), StatusKind::Deleted);
    let f = ChangedFile::from_status_columns(
        "p".into(),
        None,
        StatusKind::Deleted,
        StatusKind::Modified,
    );
    assert_eq!(f.most_severe(), StatusKind::Deleted);
}

// --- Workstream 1: git status -> XY mapping tests ---

fn find<'a>(snap: &'a RepoSnapshot, path: &str) -> &'a ChangedFile {
    snap.files
        .iter()
        .find(|f| f.path == path)
        .unwrap_or_else(|| panic!("{path} missing from snapshot"))
}

#[test]
fn snapshot_distinguishes_staged_and_unstaged_modification() {
    let (dir, path) = make_repo();
    let fp = Path::new(&path).join("a.txt");
    std::fs::write(&fp, "v1\n").unwrap();
    run_git(&path, &["add", "."]);
    run_git(&path, &["commit", "-m", "init"]);
    // Stage one modification, then modify again without staging.
    std::fs::write(&fp, "v2\n").unwrap();
    run_git(&path, &["add", "a.txt"]);
    std::fs::write(&fp, "v3\n").unwrap();

    let snap = load_snapshot(&open_repo(&path)).unwrap();
    assert_eq!(find(&snap, "a.txt").short_code(), "MM");
    drop(dir);
}

#[test]
fn snapshot_distinguishes_staged_and_unstaged_deletion() {
    let (dir, path) = make_repo();
    std::fs::write(Path::new(&path).join("staged.txt"), "x\n").unwrap();
    std::fs::write(Path::new(&path).join("wt.txt"), "y\n").unwrap();
    run_git(&path, &["add", "."]);
    run_git(&path, &["commit", "-m", "init"]);
    // staged deletion (index) vs working-tree deletion (unstaged).
    run_git(&path, &["rm", "staged.txt"]);
    std::fs::remove_file(Path::new(&path).join("wt.txt")).unwrap();

    let snap = load_snapshot(&open_repo(&path)).unwrap();
    assert_eq!(find(&snap, "staged.txt").short_code(), "D ");
    assert_eq!(find(&snap, "wt.txt").short_code(), " D");
    drop(dir);
}

#[test]
fn snapshot_keeps_staged_deletion_visible_when_path_recreated() {
    // `INDEX_DELETED | WT_NEW`: a staged deletion with a fresh untracked
    // file recreated at the same path. git emits two rows (`D ` and `??`);
    // our one-row model must keep the staged deletion rather than masking
    // the whole row as untracked.
    let (dir, path) = make_repo();
    let fp = Path::new(&path).join("f.txt");
    std::fs::write(&fp, "orig\n").unwrap();
    run_git(&path, &["add", "."]);
    run_git(&path, &["commit", "-m", "init"]);
    run_git(&path, &["rm", "--cached", "f.txt"]);
    std::fs::write(&fp, "new content\n").unwrap();

    let snap = load_snapshot(&open_repo(&path)).unwrap();
    let f = find(&snap, "f.txt");
    assert_eq!(f.index, StatusKind::Deleted);
    assert_eq!(f.short_code(), "D ");
    drop(dir);
}

#[cfg(unix)]
#[test]
fn snapshot_detects_staged_typechange() {
    let (dir, path) = make_repo();
    let fp = Path::new(&path).join("f");
    std::fs::write(&fp, "regular\n").unwrap();
    run_git(&path, &["add", "."]);
    run_git(&path, &["commit", "-m", "init"]);
    // Replace the regular file with a symlink and stage it.
    std::fs::remove_file(&fp).unwrap();
    std::os::unix::fs::symlink("target", &fp).unwrap();
    run_git(&path, &["add", "f"]);

    let snap = load_snapshot(&open_repo(&path)).unwrap();
    assert_eq!(find(&snap, "f").index, StatusKind::TypeChanged);
    assert_eq!(find(&snap, "f").short_code(), "T ");
    drop(dir);
}

#[test]
fn snapshot_renders_conflicted_file_as_uu() {
    let (dir, path) = make_repo();
    let fp = Path::new(&path).join("c.txt");
    std::fs::write(&fp, "base\n").unwrap();
    run_git(&path, &["add", "."]);
    run_git(&path, &["commit", "-m", "init"]);
    run_git(&path, &["checkout", "-b", "feature"]);
    std::fs::write(&fp, "feature\n").unwrap();
    run_git(&path, &["commit", "-am", "feature edit"]);
    run_git(&path, &["checkout", "-"]);
    std::fs::write(&fp, "mainline\n").unwrap();
    run_git(&path, &["commit", "-am", "mainline edit"]);
    // Conflicting merge exits non-zero; run it tolerantly.
    let merge = std::process::Command::new("git")
        .args(["merge", "feature"])
        .current_dir(&path)
        .output()
        .unwrap();
    assert!(!merge.status.success(), "merge should conflict");

    let snap = load_snapshot(&open_repo(&path)).unwrap();
    assert_eq!(find(&snap, "c.txt").short_code(), "UU");
    drop(dir);
}

#[test]
fn snapshot_preserves_rename_and_loads_new_side_diff() {
    let (dir, path) = make_repo();
    // Keep content identical across the rename so git's similarity
    // detection reports a staged rename rather than add+delete.
    std::fs::write(Path::new(&path).join("old.rs"), "fn main() {}\n").unwrap();
    run_git(&path, &["add", "."]);
    run_git(&path, &["commit", "-m", "init"]);
    run_git(&path, &["mv", "old.rs", "new.rs"]);

    let snap = load_snapshot(&open_repo(&path)).unwrap();
    let f = find(&snap, "new.rs");
    assert_eq!(f.index, StatusKind::Renamed);
    assert_eq!(f.old_path.as_deref(), Some("old.rs"));
    assert_eq!(f.display_path(), "old.rs -> new.rs");

    // The effective path is the new side; selecting it must still load a
    // diff without error (regression guard for the rename display change).
    assert!(load_file_diff(&open_repo(&path), &f.path).is_ok());
    drop(dir);
}
