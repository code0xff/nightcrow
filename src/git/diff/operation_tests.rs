use super::*;
use git2::Signature;

fn repo_with_commit(dir: &Path) -> Repository {
    let repo = Repository::init(dir).unwrap();
    std::fs::write(dir.join("a.txt"), "one\n").unwrap();
    let mut index = repo.index().unwrap();
    index.add_path(Path::new("a.txt")).unwrap();
    index.write().unwrap();
    let tree = repo.find_tree(index.write_tree().unwrap()).unwrap();
    let sig = Signature::now("t", "t@example.com").unwrap();
    repo.commit(Some("HEAD"), &sig, &sig, "init", &tree, &[])
        .unwrap();
    drop(tree);
    repo
}

#[test]
fn a_repository_at_rest_has_no_operation() {
    let dir = tempfile::TempDir::new().unwrap();
    let repo = repo_with_commit(dir.path());

    assert_eq!(current_operation(&repo), None);
}

#[test]
fn a_merge_head_is_a_merge_in_progress() {
    let dir = tempfile::TempDir::new().unwrap();
    let repo = repo_with_commit(dir.path());
    let head = repo.head().unwrap().target().unwrap();
    std::fs::write(repo.path().join("MERGE_HEAD"), format!("{head}\n")).unwrap();

    let op = current_operation(&repo).unwrap();
    assert_eq!(op.kind, OperationKind::Merge);
    assert_eq!(op.text(), "MERGING");
}

#[test]
fn a_rebase_reports_the_step_git_recorded() {
    let dir = tempfile::TempDir::new().unwrap();
    let repo = repo_with_commit(dir.path());
    let state = repo.path().join("rebase-merge");
    std::fs::create_dir(&state).unwrap();
    std::fs::write(state.join("msgnum"), "2\n").unwrap();
    std::fs::write(state.join("end"), "5\n").unwrap();
    std::fs::write(state.join("interactive"), "").unwrap();

    let op = current_operation(&repo).unwrap();
    assert_eq!(op.kind, OperationKind::Rebase);
    assert_eq!(op.progress, Some((2, 5)));
    assert_eq!(op.text(), "REBASING 2/5");
}

#[test]
fn a_rebase_with_unreadable_counters_is_still_shown() {
    // The state is the point; a count that cannot be trusted is left off
    // rather than shown wrong.
    let dir = tempfile::TempDir::new().unwrap();
    let repo = repo_with_commit(dir.path());
    let state = repo.path().join("rebase-merge");
    std::fs::create_dir(&state).unwrap();
    std::fs::write(state.join("msgnum"), "7\n").unwrap();
    std::fs::write(state.join("end"), "3\n").unwrap();

    let op = current_operation(&repo).unwrap();
    assert_eq!(op.kind, OperationKind::Rebase);
    assert_eq!(op.progress, None);
    assert_eq!(op.text(), "REBASING");
}

#[test]
fn a_cherry_pick_head_is_a_cherry_pick() {
    let dir = tempfile::TempDir::new().unwrap();
    let repo = repo_with_commit(dir.path());
    let head = repo.head().unwrap().target().unwrap();
    std::fs::write(repo.path().join("CHERRY_PICK_HEAD"), format!("{head}\n")).unwrap();

    assert_eq!(
        current_operation(&repo).map(|op| op.kind),
        Some(OperationKind::CherryPick)
    );
}
