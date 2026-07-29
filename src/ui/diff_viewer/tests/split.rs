use super::*;

#[test]
fn split_halves_number_the_side_each_one_shows() {
    let mut app = app_showing(skewed_pair_hunk(), DiffPaneView::Split);

    // Wide enough to clear MIN_SPLIT_WIDTH, or the renderer falls back to
    // unified and this would silently test the wrong layout.
    let screen = drawn(&mut app, 120, 10, 0);
    let joined = screen.join("\n");
    assert!(
        joined.contains("[split]"),
        "the split layout must actually be in use, got:\n{joined}"
    );

    // One row, both sides: the removed line on the left, its replacement on the
    // right. That pairing is what the split view is for.
    let row = screen
        .iter()
        .find(|l| l.contains("gone"))
        .expect("the paired change row");
    assert!(
        row.contains("fresh"),
        "the split view pairs the removal with its replacement on one row: {row:?}"
    );

    let mid = 120 / 2;
    let (left, right) = (left_columns(row, mid), right_columns(row, mid));
    assert!(
        left.contains("42") && !left.contains("77"),
        "the left half carries only the old-side number: {left:?}"
    );
    assert!(
        right.contains("77") && !right.contains("42"),
        "the right half carries only the new-side number: {right:?}"
    );
}
