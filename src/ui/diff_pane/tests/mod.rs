use super::*;
use crate::git::diff::{DiffLine, LineKind};

#[test]
fn nearest_match_index_picks_closest_and_prefers_lower_on_tie() {
    let m = [10, 30, 50];
    assert_eq!(nearest_match_index(&m, 5), 0);
    assert_eq!(nearest_match_index(&m, 10), 0);
    assert_eq!(nearest_match_index(&m, 19), 0);
    // tie: equidistant from 10 and 30 → prefer the lower row.
    assert_eq!(nearest_match_index(&m, 20), 0);
    assert_eq!(nearest_match_index(&m, 21), 1);
    assert_eq!(nearest_match_index(&m, 50), 2);
    assert_eq!(nearest_match_index(&m, 999), 2);
}

fn match_hunk(lines: &[&str]) -> DiffHunk {
    DiffHunk {
        header: "@@".to_string(),
        lines: lines
            .iter()
            .map(|s| DiffLine {
                kind: LineKind::Context,
                content: (*s).to_string(),
                old_lineno: None,
                new_lineno: None,
            })
            .collect(),
        file_path: None,
    }
}

#[test]
fn recompute_matches_keep_scroll_repins_cursor_near_viewport() {
    // 1 hunk header + 10 body lines. "foo" matches at body indices 0, 4, 8
    // → flat rows 1, 5, 9.
    let mut pane = DiffPane {
        hunks: vec![match_hunk(&[
            "foo a", "b", "c", "d", "foo e", "f", "g", "h", "foo i", "j",
        ])],
        ..Default::default()
    };
    pane.search.query.set("foo");
    pane.scroll = 6; // user is reading near the middle match (row 5)
    pane.search.cursor = 0; // stale cursor from before content changed

    pane.recompute_matches(false);

    assert_eq!(pane.search.matches, vec![1, 5, 9]);
    // Closest match to scroll=6 is row 5 (cursor index 1), not the
    // stale index 0 or a clamp to len-1.
    assert_eq!(pane.search_cursor(), 1);
    // Viewport stayed pinned where the user left it.
    assert_eq!(pane.scroll, 6);
}

#[test]
fn recompute_matches_scroll_to_match_clamps_and_jumps() {
    let mut pane = DiffPane {
        hunks: vec![match_hunk(&["foo a", "b", "foo c"])],
        ..Default::default()
    };
    pane.search.query.set("foo");
    pane.scroll = 100; // arbitrary; scroll_to_match should overwrite
    pane.search.cursor = 99; // stale, should clamp to last match index.

    pane.recompute_matches(true);

    assert_eq!(pane.search.matches, vec![1, 3]);
    assert_eq!(pane.search_cursor(), 1);
    assert_eq!(pane.scroll, 3);
}

fn kinded_hunk(lines: &[(LineKind, &str)]) -> DiffHunk {
    DiffHunk {
        header: "@@".to_string(),
        lines: lines
            .iter()
            .map(|(kind, s)| DiffLine {
                kind: *kind,
                content: (*s).to_string(),
                old_lineno: None,
                new_lineno: None,
            })
            .collect(),
        file_path: None,
    }
}

#[test]
fn split_rows_pairs_changes_and_mirrors_context() {
    use LineKind::{Added, Context, Removed};
    // A typical edit block: one context line, a 2-removed/1-added change,
    // then a trailing context line.
    let pane = DiffPane {
        hunks: vec![kinded_hunk(&[
            (Context, "ctx0"),
            (Removed, "old a"),
            (Removed, "old b"),
            (Added, "new a"),
            (Context, "ctx1"),
        ])],
        ..Default::default()
    };

    let rows = pane.split_rows();
    assert_eq!(
        rows,
        vec![
            SplitRow::Header(0),
            // context mirrored on both sides
            SplitRow::Body {
                left: Some((0, 0)),
                right: Some((0, 0)),
            },
            // removed[0] pairs with added[0]
            SplitRow::Body {
                left: Some((0, 1)),
                right: Some((0, 3)),
            },
            // removed[1] has no added counterpart → right padded blank
            SplitRow::Body {
                left: Some((0, 2)),
                right: None,
            },
            SplitRow::Body {
                left: Some((0, 4)),
                right: Some((0, 4)),
            },
        ]
    );
    // 1 header + 4 body rows.
    assert_eq!(rows.len(), 5);
}

#[test]
fn split_rows_pads_added_only_block() {
    use LineKind::Added;
    // Pure insertion: every change row has a blank left side.
    let pane = DiffPane {
        hunks: vec![kinded_hunk(&[(Added, "x"), (Added, "y")])],
        ..Default::default()
    };
    let rows = pane.split_rows();
    assert_eq!(
        rows,
        vec![
            SplitRow::Header(0),
            SplitRow::Body {
                left: None,
                right: Some((0, 0)),
            },
            SplitRow::Body {
                left: None,
                right: Some((0, 1)),
            },
        ]
    );
}

fn make_file_view_pane(content: &str) -> DiffPane {
    let mut pane = DiffPane {
        view: DiffPaneView::File,
        ..Default::default()
    };
    pane.file_view.set_content(content.to_string());
    pane
}

#[test]
fn file_view_search_matches_correct_line_indices() {
    let mut pane = make_file_view_pane("hello world\nfoo bar\nhello again\n");
    for ch in "hello".chars() {
        pane.search_push(ch);
    }
    // lines 0 and 2 contain "hello"
    assert_eq!(pane.search.matches, vec![0, 2]);
}

#[test]
fn file_view_search_no_matches() {
    let mut pane = make_file_view_pane("foo\nbar\nbaz\n");
    for ch in "xyz".chars() {
        pane.search_push(ch);
    }
    assert!(pane.search.matches.is_empty());
}

#[test]
fn file_view_search_case_insensitive() {
    let mut pane = make_file_view_pane("Hello World\nhello\nHELLO\n");
    for ch in "hello".chars() {
        pane.search_push(ch);
    }
    assert_eq!(pane.search.matches, vec![0, 1, 2]);
}

#[test]
fn file_view_next_match_updates_file_scroll() {
    let mut pane = make_file_view_pane("match\nskip\nmatch\n");
    for ch in "match".chars() {
        pane.search_push(ch);
    }
    assert_eq!(pane.file_view.scroll, 0); // jumped to first match
    pane.next_match();
    assert_eq!(pane.file_view.scroll, 2); // jumped to second match
    pane.next_match();
    assert_eq!(pane.file_view.scroll, 0); // wraps back to first
}

#[test]
fn file_view_prev_match_updates_file_scroll() {
    let mut pane = make_file_view_pane("match\nskip\nmatch\n");
    for ch in "match".chars() {
        pane.search_push(ch);
    }
    pane.prev_match();
    assert_eq!(pane.file_view.scroll, 2); // wraps to last match
}

#[test]
fn file_view_search_clear_resets_state() {
    let mut pane = make_file_view_pane("hello\nworld\n");
    for ch in "hello".chars() {
        pane.search_push(ch);
    }
    assert!(!pane.search.matches.is_empty());
    pane.cancel_search();
    assert!(pane.search.matches.is_empty());
    assert!(!pane.search.active);
}
