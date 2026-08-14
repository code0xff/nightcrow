//! What the notice row's repo header does when the names are longer than the
//! row. The counts and the recovery chip after them are the part that is news,
//! so the names are what gives way — see `fit_names`.

use crate::ui::notice::fit_names;
use ratatui::text::Span;

const PATH: &str = "~/workspace/nightcrow";
const BRANCH: &str = "feat/a-branch-named-at-some-length";

fn width(text: &str) -> usize {
    Span::raw(text).width()
}

#[test]
fn names_that_fit_are_left_alone() {
    let (path, branch) = fit_names(PATH, Some("dev"), 80);
    assert_eq!(path, format!(" {PATH} "));
    assert_eq!(branch.as_deref(), Some(" dev "));
}

#[test]
fn a_long_branch_is_cut_to_half_the_room() {
    let budget = 40;
    let (path, branch) = fit_names(PATH, Some(BRANCH), budget);
    let branch = branch.expect("a branch with room for it must still be named");
    assert!(
        branch.ends_with('…'),
        "a branch longer than its share must say it was cut, got: {branch}"
    );
    assert!(
        width(&branch) <= budget / 2,
        "the branch may take half the room, got {} of {budget}",
        width(&branch)
    );
    assert!(
        width(&path) + width(&branch) <= budget,
        "the two names together must stay inside the room left for them"
    );
}

#[test]
fn a_long_path_gives_way_before_the_branch_does() {
    // Both are longer than the row. The branch keeps its half; what is left is
    // the path's, which is the half it would have had anyway.
    let budget = 30;
    let (path, branch) = fit_names(
        "~/a/very/deeply/nested/project/directory",
        Some(BRANCH),
        budget,
    );
    let branch = branch.expect("a branch with room for it must still be named");
    assert!(path.ends_with('…'), "the path must be cut too, got: {path}");
    assert!(width(&branch) <= budget / 2);
    assert!(width(&path) + width(&branch) <= budget);
}

#[test]
fn a_row_with_no_room_for_a_branch_drops_it() {
    // An ellipsis alone names no branch, and still costs the column it is cut
    // to fit. Below two columns there is no half to give it.
    let (path, branch) = fit_names(PATH, Some(BRANCH), 1);
    assert!(branch.is_none(), "got: {branch:?}");
    assert!(width(&path) <= 1, "got: {path}");
}

#[test]
fn a_row_with_no_room_at_all_shows_neither_name() {
    // A long recovery chip on a narrow terminal can leave nothing. Even the
    // ellipsis is a column, and it would come out of the chip it was cut for.
    assert_eq!(fit_names(PATH, Some(BRANCH), 0), (String::new(), None));
    assert_eq!(fit_names(PATH, None, 0), (String::new(), None));
}

#[test]
fn a_repo_with_no_branch_gives_the_room_to_the_path() {
    // Detached HEAD and an unborn branch both arrive here as `None`.
    let (path, branch) = fit_names(PATH, None, 12);
    assert!(branch.is_none());
    assert!(path.ends_with('…'), "got: {path}");
    assert!(width(&path) <= 12);
}

#[test]
fn a_name_in_hangul_is_cut_between_characters() {
    // The budget counts columns, and each syllable takes two of them.
    let (path, branch) = fit_names("~/작업/야행성까마귀", Some("기능/한글-브랜치"), 20);
    assert!(width(&path) + branch.as_deref().map_or(0, width) <= 20);
    assert!(!path.is_empty());
}
