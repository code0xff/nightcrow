use crate::git::diff::refs::refs_fingerprint;
use crate::git::diff::{RefKind, load_commit_log, load_log_decorations};
use crate::test_util::{make_repo, open_repo, run_git};
use std::path::Path;

fn commit(path: &str, name: &str) {
    std::fs::write(Path::new(path).join(name), name).unwrap();
    run_git(path, &["add", "."]);
    run_git(path, &["commit", "-m", name]);
}

#[test]
fn head_decorates_the_branch_it_points_at() {
    let (dir, path) = make_repo();
    commit(&path, "a");
    let repo = open_repo(&path);
    let head = repo.head().unwrap().target().unwrap();

    let decorations = load_log_decorations(&repo).unwrap();

    assert!(decorations.is_head(head));
    let labels = decorations.labels_for(head);
    assert_eq!(labels.len(), 1, "{labels:?}");
    assert_eq!(labels[0].kind, RefKind::Head);
    drop(dir);
}

#[test]
fn a_tag_decorates_the_commit_it_points_at() {
    let (dir, path) = make_repo();
    commit(&path, "a");
    commit(&path, "b");
    run_git(&path, &["tag", "-a", "v1", "-m", "release", "HEAD~1"]);
    let repo = open_repo(&path);
    let commits = load_commit_log(&repo, 10).unwrap();
    let tagged = commits[1].oid;

    let decorations = load_log_decorations(&repo).unwrap();

    let labels = decorations.labels_for(tagged);
    assert!(
        labels
            .iter()
            .any(|l| l.kind == RefKind::Tag && l.name == "v1"),
        "{labels:?}"
    );
    drop(dir);
}

#[test]
fn a_commit_no_ref_points_at_gets_no_labels() {
    let (dir, path) = make_repo();
    commit(&path, "a");
    commit(&path, "b");
    let repo = open_repo(&path);
    let commits = load_commit_log(&repo, 10).unwrap();

    let decorations = load_log_decorations(&repo).unwrap();

    assert!(decorations.labels_for(commits[1].oid).is_empty());
    drop(dir);
}

#[test]
fn a_repo_with_no_upstream_marks_nothing_as_diverged() {
    let (dir, path) = make_repo();
    commit(&path, "a");
    let repo = open_repo(&path);
    let head = repo.head().unwrap().target().unwrap();

    let decorations = load_log_decorations(&repo).unwrap();

    assert!(!decorations.is_ahead(head));
    assert!(!decorations.is_behind(head));
    drop(dir);
}

#[test]
fn an_empty_repo_yields_no_decorations() {
    let (dir, path) = make_repo();

    let decorations = load_log_decorations(&open_repo(&path)).unwrap();

    assert!(decorations.labels_for(git2::Oid::ZERO_SHA1).is_empty());
    drop(dir);
}

#[test]
fn the_fingerprint_changes_when_a_ref_moves() {
    let (dir, path) = make_repo();
    commit(&path, "a");
    let before = refs_fingerprint(&open_repo(&path));

    commit(&path, "b");

    assert_ne!(before, refs_fingerprint(&open_repo(&path)));
    drop(dir);
}

#[test]
fn the_fingerprint_is_stable_when_refs_do_not_move() {
    let (dir, path) = make_repo();
    commit(&path, "a");

    let first = refs_fingerprint(&open_repo(&path));

    assert_eq!(first, refs_fingerprint(&open_repo(&path)));
    drop(dir);
}
