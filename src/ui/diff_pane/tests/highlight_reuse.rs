//! Prefix reuse for the diff highlight cache: the incremental result must be
//! identical to a full recompute for appends, edits, and structure changes.

use crate::git::diff::{DiffHunk, DiffLine, LineKind};
use crate::ui::diff_pane::{DiffPane, HIGHLIGHT_LINE_CAP, HighlightSegment};
use std::time::Instant;
use syntect::highlighting::ThemeSet;
use syntect::parsing::SyntaxSet;

fn line(kind: LineKind, content: &str) -> DiffLine {
    DiffLine {
        kind,
        content: content.to_string(),
        old_lineno: None,
        new_lineno: None,
    }
}

fn context(prefix: &str, count: usize) -> Vec<DiffLine> {
    (0..count)
        .map(|i| line(LineKind::Context, &format!("{prefix} {i}")))
        .collect()
}

fn hunk(path: Option<&str>, header: &str, lines: Vec<DiffLine>) -> DiffHunk {
    DiffHunk {
        header: header.to_string(),
        lines,
        file_path: path.map(str::to_string),
    }
}

fn full_recompute(
    hunks: Vec<DiffHunk>,
    ss: &SyntaxSet,
    ts: &ThemeSet,
) -> Vec<Vec<Vec<HighlightSegment>>> {
    let mut fresh = DiffPane::default();
    fresh.set_hunks(hunks);
    fresh.ensure_highlight_cache(ss, ts);
    fresh.line_highlights
}

fn assert_matches_full(pane: &DiffPane, hunks: Vec<DiffHunk>, ss: &SyntaxSet, ts: &ThemeSet) {
    let expected = full_recompute(hunks, ss, ts);
    assert_eq!(pane.line_highlights, expected);
}

/// A block comment makes the highlight state cross line boundaries, so a
/// resumed prefix that skipped the comment would visibly diverge.
fn rust_lines(count: usize) -> Vec<DiffLine> {
    let mut lines = vec![line(LineKind::Context, "/* stateful comment")];
    lines.extend((0..count).map(|i| line(LineKind::Context, &format!("still comment {i}"))));
    lines.push(line(LineKind::Context, "*/"));
    lines
}

#[test]
fn appending_to_the_last_hunk_matches_a_full_recompute() {
    let ss = two_face::syntax::extra_newlines();
    let ts = ThemeSet::load_defaults();

    let base = rust_lines(20);
    let mut pane = DiffPane::default();
    pane.set_hunks(vec![hunk(Some("demo.rs"), "@@ -1 +1 @@", base.clone())]);
    pane.ensure_highlight_cache(&ss, &ts);

    let mut appended = base;
    appended.extend((0..5).map(|i| line(LineKind::Added, &format!("let added_{i} = {i};"))));
    // Appending changes the hunk's line range, so its header differs from the
    // previous generation while the leading lines are still shared.
    let hunks = vec![hunk(Some("demo.rs"), "@@ -1 +30 @@", appended)];
    pane.set_hunks(hunks.clone());
    pane.ensure_highlight_cache(&ss, &ts);

    assert_matches_full(&pane, hunks, &ss, &ts);
}

#[test]
fn appending_across_the_cap_matches_a_full_recompute() {
    let ss = two_face::syntax::extra_newlines();
    let ts = ThemeSet::load_defaults();

    let base = context("line", HIGHLIGHT_LINE_CAP - 5);
    let mut pane = DiffPane::default();
    pane.set_hunks(vec![hunk(None, "@@ -1 +1 @@", base.clone())]);
    pane.ensure_highlight_cache(&ss, &ts);

    let mut appended = base;
    appended.extend(context("tail", 20));
    let hunks = vec![hunk(None, "@@ -1 +4010 @@", appended)];
    pane.set_hunks(hunks.clone());
    pane.ensure_highlight_cache(&ss, &ts);

    assert_eq!(pane.line_highlights[0].len(), HIGHLIGHT_LINE_CAP);
    assert_matches_full(&pane, hunks, &ss, &ts);
}

#[test]
fn appending_to_an_already_full_cap_matches_a_full_recompute() {
    let ss = two_face::syntax::extra_newlines();
    let ts = ThemeSet::load_defaults();

    let base = context("line", HIGHLIGHT_LINE_CAP);
    let mut pane = DiffPane::default();
    pane.set_hunks(vec![hunk(None, "@@ -1 +1 @@", base.clone())]);
    pane.ensure_highlight_cache(&ss, &ts);

    let mut appended = base;
    appended.extend(context("tail", 10));
    let hunks = vec![hunk(None, "@@ -1 +4010 @@", appended)];
    pane.set_hunks(hunks.clone());
    pane.ensure_highlight_cache(&ss, &ts);

    assert_eq!(pane.line_highlights[0].len(), HIGHLIGHT_LINE_CAP);
    assert_matches_full(&pane, hunks, &ss, &ts);
}

#[test]
fn a_middle_edit_matches_a_full_recompute() {
    let ss = two_face::syntax::extra_newlines();
    let ts = ThemeSet::load_defaults();

    let first = hunk(Some("demo.rs"), "@@ -1 +1 @@", rust_lines(8));
    let mut middle = hunk(Some("demo.rs"), "@@ -10 +10 @@", rust_lines(8));
    let mut last = hunk(Some("demo.rs"), "@@ -20 +20 @@", rust_lines(8));
    let mut pane = DiffPane::default();
    pane.set_hunks(vec![first.clone(), middle.clone(), last.clone()]);
    pane.ensure_highlight_cache(&ss, &ts);

    middle.lines[4] = line(LineKind::Added, "let changed = true;");
    last.lines.push(line(LineKind::Added, "let tail = 1;"));
    let hunks = vec![first, middle, last];
    pane.set_hunks(hunks.clone());
    pane.ensure_highlight_cache(&ss, &ts);

    assert_matches_full(&pane, hunks, &ss, &ts);
}

#[test]
fn a_split_hunk_matches_a_full_recompute() {
    let ss = two_face::syntax::extra_newlines();
    let ts = ThemeSet::load_defaults();

    let first = hunk(Some("demo.rs"), "@@ -1 +1 @@", rust_lines(6));
    let second = hunk(Some("demo.rs"), "@@ -20 +20 @@", rust_lines(6));
    let mut pane = DiffPane::default();
    pane.set_hunks(vec![first.clone(), second.clone()]);
    pane.ensure_highlight_cache(&ss, &ts);

    let mut split_left = second.lines.clone();
    let split_right = split_left.split_off(3);
    let hunks = vec![
        first,
        hunk(Some("demo.rs"), "@@ -20 +20 @@", split_left),
        hunk(Some("demo.rs"), "@@ -23 +23 @@", split_right),
    ];
    pane.set_hunks(hunks.clone());
    pane.ensure_highlight_cache(&ss, &ts);

    assert_matches_full(&pane, hunks, &ss, &ts);
}

#[test]
fn a_syntax_change_at_the_boundary_matches_a_full_recompute() {
    let ss = two_face::syntax::extra_newlines();
    let ts = ThemeSet::load_defaults();

    let rust = hunk(Some("demo.rs"), "@@ -1 +1 @@", rust_lines(6));
    let js = hunk(
        Some("demo.js"),
        "@@ -1 +1 @@",
        vec![line(LineKind::Context, "/* js comment")],
    );
    let mut pane = DiffPane::default();
    pane.set_hunks(vec![rust.clone(), js]);
    pane.ensure_highlight_cache(&ss, &ts);

    // Same lines, but the second file now resolves to Rust: the full pass
    // resets both highlighters at that boundary.
    let retargeted = hunk(
        Some("demo.rs"),
        "@@ -1 +1 @@",
        vec![line(LineKind::Context, "/* js comment")],
    );
    let hunks = vec![rust, retargeted];
    pane.set_hunks(hunks.clone());
    pane.ensure_highlight_cache(&ss, &ts);

    assert_matches_full(&pane, hunks, &ss, &ts);
}

#[test]
#[ignore = "timing assertion; run explicitly with --ignored"]
fn appending_highlights_only_the_new_tail() {
    let ss = two_face::syntax::extra_newlines();
    let ts = ThemeSet::load_defaults();

    let base = context("line", HIGHLIGHT_LINE_CAP);
    let mut pane = DiffPane::default();
    pane.set_hunks(vec![hunk(None, "@@ -1 +1 @@", base.clone())]);
    pane.ensure_highlight_cache(&ss, &ts);

    let mut appended = base;
    appended.extend(context("tail", 10));
    let hunks = vec![hunk(None, "@@ -1 +1 @@", appended)];

    pane.set_hunks(hunks.clone());
    let append_start = Instant::now();
    pane.ensure_highlight_cache(&ss, &ts);
    let append = append_start.elapsed();

    let mut fresh = DiffPane::default();
    fresh.set_hunks(hunks);
    let full_start = Instant::now();
    fresh.ensure_highlight_cache(&ss, &ts);
    let full = full_start.elapsed();

    println!("append {append:?}, full {full:?}");
    assert!(
        append * 4 < full,
        "append {append:?} should be far below a full pass {full:?}"
    );
}
