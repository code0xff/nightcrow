//! Boundaries of the synchronous highlight pass and the lazy search cache.

use crate::git::diff::{DiffHunk, DiffLine, LineKind};
use crate::ui::diff_pane::{DiffPane, DiffPaneView, HIGHLIGHT_LINE_CAP};

fn numbered_lines(count: usize) -> Vec<String> {
    (0..count).map(|i| format!("line {i}")).collect()
}

fn context_hunk(lines: &[String]) -> DiffHunk {
    DiffHunk {
        header: "@@ -1 +1 @@".to_string(),
        lines: lines
            .iter()
            .map(|text| DiffLine {
                kind: LineKind::Context,
                content: text.clone(),
                old_lineno: None,
                new_lineno: None,
            })
            .collect(),
        file_path: None,
    }
}

fn highlighted_body_lines(pane: &DiffPane) -> usize {
    pane.line_highlights.iter().map(Vec::len).sum()
}

#[test]
fn highlight_cache_stops_at_the_line_cap() {
    let ss = two_face::syntax::extra_newlines();
    let ts = syntect::highlighting::ThemeSet::load_defaults();
    for count in [
        HIGHLIGHT_LINE_CAP - 1,
        HIGHLIGHT_LINE_CAP,
        HIGHLIGHT_LINE_CAP + 1,
    ] {
        let mut pane = DiffPane::default();
        pane.set_hunks(vec![context_hunk(&numbered_lines(count))]);
        pane.ensure_highlight_cache(&ss, &ts);
        assert_eq!(
            highlighted_body_lines(&pane),
            count.min(HIGHLIGHT_LINE_CAP),
            "{count} body lines"
        );
    }
}

#[test]
fn a_hunk_straddling_the_cap_keeps_only_its_in_budget_prefix() {
    let ss = two_face::syntax::extra_newlines();
    let ts = syntect::highlighting::ThemeSet::load_defaults();
    let half = HIGHLIGHT_LINE_CAP / 2;
    let mut pane = DiffPane::default();
    pane.set_hunks(vec![
        context_hunk(&numbered_lines(half)),
        context_hunk(&numbered_lines(half + 10)),
        context_hunk(&numbered_lines(1)),
    ]);
    pane.ensure_highlight_cache(&ss, &ts);

    assert_eq!(pane.line_highlights.len(), 2);
    assert_eq!(pane.line_highlights[0].len(), half);
    assert_eq!(
        pane.line_highlights[1].len(),
        HIGHLIGHT_LINE_CAP - half,
        "the straddling hunk keeps its in-budget prefix"
    );
}

#[test]
fn lower_cache_is_not_built_until_search_runs() {
    let mut pane = DiffPane::default();
    pane.set_hunks(vec![context_hunk(&[
        "Alpha".to_string(),
        "BETA".to_string(),
    ])]);
    assert!(pane.hunks_lines_lower.is_empty());

    pane.search.query.set("beta");
    pane.recompute_matches(false);
    assert_eq!(pane.hunks_lines_lower[0], vec!["alpha", "beta"]);
}

#[test]
fn search_finds_a_match_past_the_highlight_cap() {
    let ss = two_face::syntax::extra_newlines();
    let ts = syntect::highlighting::ThemeSet::load_defaults();
    let mut lines = numbered_lines(HIGHLIGHT_LINE_CAP + 3);
    let last = lines.len() - 1;
    lines[last] = "needle past the cap".to_string();

    let mut pane = DiffPane::default();
    pane.set_hunks(vec![context_hunk(&lines)]);
    pane.ensure_highlight_cache(&ss, &ts);
    assert_eq!(highlighted_body_lines(&pane), HIGHLIGHT_LINE_CAP);

    for ch in "needle".chars() {
        pane.search_push(ch);
    }
    // One hunk header precedes body line 0, so the flat row is `last + 1`.
    assert_eq!(pane.search.matches, vec![last + 1]);
    assert_eq!(pane.scroll, last + 1);
}

#[test]
fn file_view_search_finds_a_match_past_the_highlight_cap() {
    let mut lines = numbered_lines(HIGHLIGHT_LINE_CAP + 2);
    let last = lines.len() - 1;
    lines[last] = "needle".to_string();

    let mut pane = DiffPane {
        view: DiffPaneView::File,
        ..Default::default()
    };
    pane.file_view.set_content(lines.join("\n"));
    for ch in "needle".chars() {
        pane.search_push(ch);
    }

    assert_eq!(pane.search.matches, vec![last]);
    assert_eq!(
        pane.file_view.scroll,
        last.min(pane.file_view.max_scroll()),
        "the viewport follows the match past the cap"
    );
}

#[test]
fn split_layout_still_has_rows_past_the_highlight_cap() {
    let mut pane = DiffPane {
        view: DiffPaneView::Split,
        ..Default::default()
    };
    pane.set_hunks(vec![context_hunk(&numbered_lines(HIGHLIGHT_LINE_CAP + 1))]);
    // One header row plus one mirrored row per context line: the layout is
    // never truncated by the highlight cap.
    assert_eq!(pane.split_rows().len(), HIGHLIGHT_LINE_CAP + 2);
}
