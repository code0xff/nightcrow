use super::*;

/// One context line far wider than any pane this test renders into.
fn long_line_hunk() -> DiffHunk {
    DiffHunk {
        header: "@@ -7,1 +7,1 @@".to_string(),
        lines: vec![DiffLine {
            kind: LineKind::Context,
            content: "alpha bravo charlie delta echo foxtrot golf hotel india juliett".to_string(),
            old_lineno: Some(7),
            new_lineno: Some(7),
        }],
        file_path: Some("src/lib.rs".to_string()),
    }
}

/// Rows that carry any of the line's words, i.e. how many screen rows the one
/// logical line ended up occupying.
fn rows_with_content(screen: &[String]) -> usize {
    screen
        .iter()
        .filter(|l| {
            ["alpha", "charlie", "foxtrot", "india", "juliett"]
                .iter()
                .any(|w| l.contains(w))
        })
        .count()
}

#[test]
fn wrapping_folds_a_long_line_onto_several_rows() {
    let mut app = app_showing(long_line_hunk(), DiffPaneView::Diff);

    let truncated = drawn(&mut app, 40, 10, 0);
    app.git.view.diff.wrap = true;
    let wrapped = drawn(&mut app, 40, 10, 0);

    assert_eq!(
        rows_with_content(&truncated),
        1,
        "unwrapped, the line is clipped to one row:\n{truncated:#?}"
    );
    assert!(
        rows_with_content(&wrapped) > 1,
        "wrapped, it must continue onto further rows:\n{wrapped:#?}"
    );
    assert!(
        wrapped.iter().any(|l| l.contains("juliett")),
        "the tail must become reachable without scrolling:\n{wrapped:#?}"
    );
}

/// With wrapping on the gutter is folded into the body line, because a separate
/// gutter paragraph would desynchronise the moment a line spans two rows.
#[test]
fn wrapping_keeps_the_line_number_on_the_row_the_line_starts_on() {
    let mut app = app_showing(long_line_hunk(), DiffPaneView::Diff);
    app.git.view.diff.wrap = true;

    let screen = drawn(&mut app, 40, 10, 0);
    let first = screen
        .iter()
        .find(|l| l.contains("alpha"))
        .expect("the row the line starts on");

    assert!(
        first.contains('7'),
        "the number travels with its own line: {first:?}"
    );
    let continuation = screen
        .iter()
        .find(|l| l.contains("juliett") && !l.contains("alpha"))
        .expect("a continuation row");
    assert!(
        !continuation.contains('7'),
        "a continuation row must not be numbered again: {continuation:?}"
    );
}

#[test]
fn the_split_view_ignores_wrapping() {
    // Halves that fold to different heights would stop lining up, and lining up
    // is the only reason to be in this layout.
    let mut app = app_showing(long_line_hunk(), DiffPaneView::Split);
    app.git.view.diff.wrap = true;

    let screen = drawn(&mut app, 120, 10, 0);

    assert_eq!(
        rows_with_content(&screen),
        1,
        "the split row stays clipped to one row:\n{screen:#?}"
    );
}
