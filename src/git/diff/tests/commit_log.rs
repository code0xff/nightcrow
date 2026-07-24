use crate::git::diff::{
    head_commit_oid, is_empty_head, load_commit_log, load_commit_log_from, load_commit_log_page,
};
use crate::test_util::{make_repo, open_repo, run_git};
use git2::Oid;
use std::path::Path;

#[test]
fn commit_log_empty_repo_returns_empty() {
    let (dir, path) = make_repo();

    let commits = load_commit_log(&open_repo(&path), 10).unwrap();

    assert!(commits.is_empty());
    drop(dir);
}

#[test]
fn commit_log_page_empty_repo_returns_empty() {
    let (dir, path) = make_repo();

    let page = load_commit_log_page(&open_repo(&path), 0, 5).unwrap();

    assert!(page.is_empty());
    drop(dir);
}

#[test]
fn commit_log_page_zero_limit_returns_empty() {
    let (dir, path) = make_repo();
    std::fs::write(Path::new(&path).join("f"), "x").unwrap();
    run_git(&path, &["add", "."]);
    run_git(&path, &["commit", "-m", "c1"]);

    let page = load_commit_log_page(&open_repo(&path), 0, 0).unwrap();

    assert!(page.is_empty());
    drop(dir);
}

#[test]
fn commit_log_page_paginates_via_skip() {
    let (dir, path) = make_repo();
    for i in 0..5 {
        std::fs::write(Path::new(&path).join(format!("f{i}")), format!("{i}")).unwrap();
        run_git(&path, &["add", "."]);
        run_git(&path, &["commit", "-m", &format!("c{i}")]);
    }

    let first = load_commit_log_page(&open_repo(&path), 0, 2).unwrap();
    let second = load_commit_log_page(&open_repo(&path), 2, 2).unwrap();
    let third = load_commit_log_page(&open_repo(&path), 4, 2).unwrap();

    // Newest first: c4, c3 | c2, c1 | c0.
    assert_eq!(first.len(), 2);
    assert_eq!(first[0].summary, "c4");
    assert_eq!(first[1].summary, "c3");
    assert_eq!(second.len(), 2);
    assert_eq!(second[0].summary, "c2");
    assert_eq!(second[1].summary, "c1");
    assert_eq!(third.len(), 1);
    assert_eq!(third[0].summary, "c0");
    drop(dir);
}

#[test]
fn commit_log_from_an_anchor_ignores_commits_made_after_it() {
    // The point of the anchor: a page fetched after a new commit landed must
    // continue the history the first page described, not a shifted one.
    let (dir, path) = make_repo();
    for i in 0..4 {
        std::fs::write(Path::new(&path).join(format!("f{i}")), format!("{i}")).unwrap();
        run_git(&path, &["add", "."]);
        run_git(&path, &["commit", "-m", &format!("c{i}")]);
    }
    let first = load_commit_log_from(&open_repo(&path), None, 0, 2).unwrap();
    let anchor = first[0].oid;

    // A commit lands between the two page requests.
    std::fs::write(Path::new(&path).join("late"), "x").unwrap();
    run_git(&path, &["add", "."]);
    run_git(&path, &["commit", "-m", "late"]);

    let anchored = load_commit_log_from(&open_repo(&path), Some(anchor), 2, 2).unwrap();
    let unanchored = load_commit_log_from(&open_repo(&path), None, 2, 2).unwrap();

    // Anchored: c3, c2 | c1, c0 — one history, no repeats.
    assert_eq!(first[0].summary, "c3");
    assert_eq!(first[1].summary, "c2");
    assert_eq!(
        anchored.iter().map(|c| c.summary.as_str()).collect::<Vec<_>>(),
        ["c1", "c0"],
    );
    // Unanchored: HEAD moved, so the same skip lands a row late and repeats
    // a commit the caller already has. This is what the anchor prevents.
    assert_eq!(
        unanchored
            .iter()
            .map(|c| c.summary.as_str())
            .collect::<Vec<_>>(),
        ["c2", "c1"],
    );
    drop(dir);
}

#[test]
fn commit_log_from_a_missing_anchor_is_an_error() {
    let (dir, path) = make_repo();
    std::fs::write(Path::new(&path).join("f"), "x").unwrap();
    run_git(&path, &["add", "."]);
    run_git(&path, &["commit", "-m", "only"]);
    let absent = Oid::from_str("0123456789012345678901234567890123456789").unwrap();

    let result = load_commit_log_from(&open_repo(&path), Some(absent), 0, 5);

    assert!(result.is_err(), "an unknown anchor must not walk from HEAD");
    drop(dir);
}

#[test]
fn commit_log_from_a_missing_anchor_is_an_error_in_an_empty_repo_too() {
    // The emptiness check must not short-circuit ahead of the anchor: an
    // unknown commit is the caller's error, and answering "no history"
    // instead would report a typo as an exhausted log.
    let (dir, path) = make_repo();
    let absent = Oid::from_str("0123456789012345678901234567890123456789").unwrap();

    let result = load_commit_log_from(&open_repo(&path), Some(absent), 0, 5);

    assert!(result.is_err());
    drop(dir);
}

#[test]
fn head_commit_oid_separates_an_unborn_head_from_a_commit() {
    let (dir, path) = make_repo();
    assert_eq!(head_commit_oid(&open_repo(&path)).unwrap(), None);

    std::fs::write(Path::new(&path).join("f"), "x").unwrap();
    run_git(&path, &["add", "."]);
    run_git(&path, &["commit", "-m", "only"]);

    let oid = head_commit_oid(&open_repo(&path)).unwrap();

    assert!(oid.is_some(), "a committed repository has a HEAD target");
    drop(dir);
}

#[test]
fn head_commit_oid_reports_an_unreadable_head_rather_than_no_history() {
    let (dir, path) = make_repo();
    std::fs::write(Path::new(&path).join("f"), "x").unwrap();
    run_git(&path, &["add", "."]);
    run_git(&path, &["commit", "-m", "only"]);
    let repo = open_repo(&path);
    // Not a ref line at all. A HEAD naming a branch that does not exist is
    // *not* the case to test: libgit2 calls that an unborn branch, and
    // rightly — nothing in the repository distinguishes a branch not yet
    // created from one deleted afterwards. Only unparseable content is a
    // broken repository.
    std::fs::write(Path::new(&path).join(".git/HEAD"), "not-a-ref\n").unwrap();

    let result = head_commit_oid(&repo);

    assert!(
        result.is_err(),
        "an unreadable HEAD must not read as an empty history"
    );
    drop(dir);
}

#[test]
fn commit_log_page_skip_beyond_history_returns_empty() {
    let (dir, path) = make_repo();
    std::fs::write(Path::new(&path).join("f"), "x").unwrap();
    run_git(&path, &["add", "."]);
    run_git(&path, &["commit", "-m", "only"]);

    let page = load_commit_log_page(&open_repo(&path), 5, 10).unwrap();

    assert!(page.is_empty());
    drop(dir);
}

#[test]
fn is_empty_head_recognizes_unborn_branch_error() {
    // Drive the actual error path: a freshly-initialized repo has no
    // HEAD target, so revwalk.push_head() returns the error variant our
    // helper must recognize. This guards against libgit2 changing the
    // error class/code combination it reports.
    let (dir, path) = make_repo();
    let repo = open_repo(&path);
    let mut revwalk = repo.revwalk().unwrap();
    let err = revwalk
        .push_head()
        .expect_err("empty repo should fail to push HEAD");
    assert!(
        is_empty_head(&err),
        "is_empty_head failed to recognize unborn HEAD error: \
         class={:?} code={:?} message={}",
        err.class(),
        err.code(),
        err.message()
    );
    drop(dir);
}