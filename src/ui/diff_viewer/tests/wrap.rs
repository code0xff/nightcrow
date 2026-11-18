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

#[test]
fn wrapping_aligns_continuations_and_keeps_numbers_with_their_lines() {
    let mut hunk = long_line_hunk();
    hunk.lines[0].kind = LineKind::Added;
    hunk.lines[0].old_lineno = None;
    hunk.lines.push(DiffLine {
        kind: LineKind::Added,
        content: "next row".to_string(),
        old_lineno: None,
        new_lineno: Some(8),
    });
    let mut app = app_showing(hunk, DiffPaneView::Diff);
    app.git.view.diff.wrap = true;

    let screen = drawn(&mut app, 40, 10, 0);
    let first = screen
        .iter()
        .find(|l| l.contains("alpha"))
        .expect("the row the line starts on");

    assert!(
        first.contains("7 +alpha"),
        "the number travels with its own line: {first:?}"
    );
    let continuation = screen
        .iter()
        .find(|l| l.contains("juliett") && !l.contains("alpha"))
        .expect("a continuation row");
    let body_column = col_of(first, "alpha");
    assert_eq!(
        left_columns(continuation, body_column),
        format!("│{}", " ".repeat(body_column - 1)),
        "the continuation must leave the gutter and marker columns blank"
    );
    assert!(screen.iter().any(|row| row.contains("8 +next row")));
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
