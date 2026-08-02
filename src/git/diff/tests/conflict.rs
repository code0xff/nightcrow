//! What a merge conflict looks like through the diff loaders.

use crate::git::diff::load_file_diff;
use crate::test_util::{make_repo, open_repo, run_git, run_git_expecting_failure};
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
    run_git_expecting_failure(&path, &["merge", "other"]);

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
