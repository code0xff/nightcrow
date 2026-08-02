//! What a merge conflict looks like through the diff loaders.

use crate::git::diff::load_file_diff;
use crate::test_util::{make_repo, open_repo, run_git, run_git_expecting_conflict};
use std::path::Path;

/// A conflicted file has the most to say and used to say nothing.
///
/// Its path has no stage-0 index entry, so the index-aware diff answers with a
/// delta and no hunks at all — the status list showed `UU`, clicking it showed
/// an empty pane, and empty is what an unchanged file looks like too.
#[test]
fn a_conflicted_file_shows_the_conflict() {
    let (dir, path) = make_repo();
    let file = Path::new(&path).join("c.txt");
    std::fs::write(&file, "base\n").unwrap();
    run_git(&path, &["add", "."]);
    run_git(&path, &["commit", "-m", "base"]);
    run_git(&path, &["checkout", "-b", "other"]);
    std::fs::write(&file, "theirs\n").unwrap();
    run_git(&path, &["commit", "-am", "theirs"]);
    run_git(&path, &["checkout", "-"]);
    std::fs::write(&file, "ours\n").unwrap();
    run_git(&path, &["commit", "-am", "ours"]);
    run_git_expecting_conflict(&path, &["merge", "other"]);

    let hunks = load_file_diff(&open_repo(&path), "c.txt").unwrap();
    let text: String = hunks
        .iter()
        .flat_map(|hunk| hunk.lines.iter())
        .map(|line| line.content.as_str())
        .collect::<Vec<_>>()
        .join("\n");
    assert!(
        text.contains("<<<<<<<") && text.contains("theirs"),
        "a conflicted file answered without its conflict: {text:?}"
    );
    drop(dir);
}

/// A conflict with nothing to diff still has something to say.
///
/// git keeps our version byte for byte in a modify/delete, so HEAD and the
/// working tree agree and there is no text to show — the pane was blank for a
/// row the status list shows as unmerged. What is worth reading there is the
/// shape of the conflict, which the index still holds.
#[test]
fn a_conflict_without_markers_names_itself() {
    let (dir, path) = make_repo();
    let file = Path::new(&path).join("keep.txt");
    std::fs::write(&file, "base\n").unwrap();
    run_git(&path, &["add", "."]);
    run_git(&path, &["commit", "-m", "base"]);
    run_git(&path, &["checkout", "-b", "other"]);
    std::fs::remove_file(&file).unwrap();
    run_git(&path, &["commit", "-am", "they delete it"]);
    run_git(&path, &["checkout", "-"]);
    std::fs::write(&file, "ours\n").unwrap();
    run_git(&path, &["commit", "-am", "we modify it"]);
    run_git_expecting_conflict(&path, &["merge", "other"]);

    let hunks = load_file_diff(&open_repo(&path), "keep.txt").unwrap();
    let text = hunks
        .iter()
        .flat_map(|hunk| hunk.lines.iter())
        .map(|line| line.content.as_str())
        .collect::<Vec<_>>()
        .join("\n");
    assert!(
        text.contains("deleted by them"),
        "a modify/delete conflict answered with nothing to read: {hunks:?}"
    );
    drop(dir);
}

/// A path both sides created has no common version, and must not be described
/// as though it had one.
///
/// Binary, because that is what reaches the summary: a text add/add gets
/// markers written into it and diffs like anything else. git keeps ours byte
/// for byte here, so the summary is the only thing that says what happened.
#[test]
fn a_path_both_sides_added_is_not_called_modified() {
    let (dir, path) = make_repo();
    let file = Path::new(&path).join("a.bin");
    std::fs::write(Path::new(&path).join("seed.txt"), "seed\n").unwrap();
    run_git(&path, &["add", "."]);
    run_git(&path, &["commit", "-m", "seed"]);
    run_git(&path, &["checkout", "-b", "other"]);
    std::fs::write(&file, [0u8, 159, 146, 150]).unwrap();
    run_git(&path, &["add", "-A"]);
    run_git(&path, &["commit", "-m", "they add it"]);
    run_git(&path, &["checkout", "-"]);
    std::fs::write(&file, [0u8, 1, 2, 3]).unwrap();
    run_git(&path, &["add", "-A"]);
    run_git(&path, &["commit", "-m", "we add it"]);
    run_git_expecting_conflict(&path, &["merge", "other"]);

    let hunks = load_file_diff(&open_repo(&path), "a.bin").unwrap();
    let text = hunks
        .iter()
        .flat_map(|hunk| hunk.lines.iter())
        .map(|line| line.content.as_str())
        .collect::<Vec<_>>()
        .join("\n");
    assert!(
        text.contains("both added"),
        "a path neither side had before was described as: {text:?}"
    );
    drop(dir);
}
