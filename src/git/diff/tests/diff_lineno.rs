use crate::git::diff::{LineKind, load_commit_file_diff, load_commit_log, load_file_diff};
use crate::test_util::{make_repo, open_repo, run_git};
use std::path::Path;

/// `(kind, old_lineno, new_lineno)` for every line of a hunk — the shape the
/// gutter renderer will consume.
fn gutter(hunk: &crate::git::diff::DiffHunk) -> Vec<(LineKind, Option<u32>, Option<u32>)> {
    hunk.lines
        .iter()
        .map(|l| (l.kind, l.old_lineno, l.new_lineno))
        .collect()
}

#[test]
fn mixed_hunk_lines_carry_the_line_numbers_of_the_side_they_exist_on() {
    let (dir, path) = make_repo();
    let fp = Path::new(&path).join("m.txt");
    std::fs::write(&fp, "one\ntwo\nthree\n").unwrap();
    run_git(&path, &["add", "."]);
    run_git(&path, &["commit", "-m", "init"]);
    std::fs::write(&fp, "one\nTWO\nthree\n").unwrap();

    let hunks = load_file_diff(&open_repo(&path), "m.txt").unwrap();

    assert_eq!(hunks.len(), 1);
    assert_eq!(
        gutter(&hunks[0]),
        vec![
            (LineKind::Context, Some(1), Some(1)),
            // Removed exists only on the old side, added only on the new side.
            (LineKind::Removed, Some(2), None),
            (LineKind::Added, None, Some(2)),
            (LineKind::Context, Some(3), Some(3)),
        ]
    );
    drop(dir);
}

#[test]
fn second_hunk_resumes_from_its_own_offsets_after_an_earlier_insertion() {
    let (dir, path) = make_repo();
    let fp = Path::new(&path).join("long.txt");
    let before: String = (1..=20).map(|n| format!("l{n:02}\n")).collect();
    std::fs::write(&fp, &before).unwrap();
    run_git(&path, &["add", "."]);
    run_git(&path, &["commit", "-m", "init"]);
    // Insert one extra line near the top and edit near the bottom, so the two
    // changes land in separate hunks and the new side of the second hunk is
    // offset by one from the old side.
    let after: String = (1..=20)
        .map(|n| match n {
            2 => "l02a\nl02b\n".to_string(),
            18 => "l18x\n".to_string(),
            _ => format!("l{n:02}\n"),
        })
        .collect();
    std::fs::write(&fp, &after).unwrap();

    let hunks = load_file_diff(&open_repo(&path), "long.txt").unwrap();

    assert_eq!(hunks.len(), 2, "expected two separate hunks");
    let second = gutter(&hunks[1]);
    // First context line of the second hunk: old side 15, new side 16 because
    // the earlier hunk added a line.
    assert_eq!(second[0], (LineKind::Context, Some(15), Some(16)));
    assert_eq!(
        second,
        vec![
            (LineKind::Context, Some(15), Some(16)),
            (LineKind::Context, Some(16), Some(17)),
            (LineKind::Context, Some(17), Some(18)),
            (LineKind::Removed, Some(18), None),
            (LineKind::Added, None, Some(19)),
            (LineKind::Context, Some(19), Some(20)),
            (LineKind::Context, Some(20), Some(21)),
        ]
    );
    drop(dir);
}

#[test]
fn commit_diff_lines_carry_line_numbers_too() {
    let (dir, path) = make_repo();
    let fp = Path::new(&path).join("c.txt");
    std::fs::write(&fp, "one\ntwo\n").unwrap();
    run_git(&path, &["add", "."]);
    run_git(&path, &["commit", "-m", "init"]);
    std::fs::write(&fp, "one\nTWO\n").unwrap();
    run_git(&path, &["add", "."]);
    run_git(&path, &["commit", "-m", "edit"]);

    let commits = load_commit_log(&open_repo(&path), 1).unwrap();
    let hunks = load_commit_file_diff(&open_repo(&path), commits[0].oid, "c.txt").unwrap();

    // The commit collector prepends a synthetic per-file header hunk; the real
    // hunk follows it and must still carry libgit2's numbers.
    let real = hunks
        .iter()
        .find(|h| h.header.starts_with("@@"))
        .expect("commit diff should contain a unified hunk");
    assert_eq!(
        gutter(real),
        vec![
            (LineKind::Context, Some(1), Some(1)),
            (LineKind::Removed, Some(2), None),
            (LineKind::Added, None, Some(2)),
        ]
    );
    drop(dir);
}
