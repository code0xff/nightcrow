//! Line-number gutter rendering.
//!
//! The gutter and the diff body are deliberately two `Paragraph`s: a single
//! paragraph would slide the numbers off the left edge as soon as the body is
//! scrolled horizontally. Several of these tests exist only to keep that from
//! regressing, so they assert on a rendered screen rather than on a helper.

use crate::app::tests::app_with_files;
use crate::app::{App, DiffPaneView, ViewMode};
use crate::git::diff::{DiffHunk, DiffLine, LineKind};
use ratatui::{Terminal, backend::TestBackend, layout::Rect, style::Color};
use std::time::Instant;
use syntect::highlighting::ThemeSet;

/// A context / removed / added trio, which is the shape that exercises every
/// gutter column state: both numbers, old only, new only.
fn trio_hunk() -> DiffHunk {
    DiffHunk {
        header: "@@ -41,3 +41,3 @@".to_string(),
        lines: vec![
            DiffLine {
                kind: LineKind::Context,
                content: "keep_me();".to_string(),
                old_lineno: Some(41),
                new_lineno: Some(41),
            },
            DiffLine {
                kind: LineKind::Removed,
                content: "gone();".to_string(),
                old_lineno: Some(42),
                new_lineno: None,
            },
            DiffLine {
                kind: LineKind::Added,
                content: "fresh();".to_string(),
                old_lineno: None,
                new_lineno: Some(42),
            },
        ],
        file_path: Some("src/lib.rs".to_string()),
    }
}

fn app_showing(hunk: DiffHunk, view: DiffPaneView) -> App {
    let mut app = app_with_files(vec!["src/lib.rs"]);
    app.mode = ViewMode::Status;
    app.diff.set_hunks(vec![hunk]);
    app.diff.view = view;
    app
}

#[test]
#[ignore = "release performance benchmark"]
fn repeated_large_split_render_reuses_mutation_caches() {
    let lines: Vec<DiffLine> = (1..=20_000)
        .map(|line_no| DiffLine {
            kind: LineKind::Context,
            content: format!("line {line_no}"),
            old_lineno: Some(line_no),
            new_lineno: Some(line_no),
        })
        .collect();
    let mut app = app_showing(
        DiffHunk {
            header: "@@ -1,20000 +1,20000 @@".to_string(),
            lines,
            file_path: Some("src/lib.rs".to_string()),
        },
        DiffPaneView::Split,
    );
    let width = 120;
    let height = 12;
    let ss = two_face::syntax::extra_newlines();
    let ts = ThemeSet::load_defaults();
    let mut terminal = Terminal::new(TestBackend::new(width, height)).expect("a terminal");

    terminal
        .draw(|frame| {
            super::render(
                frame,
                &mut app,
                Rect::new(0, 0, width, height),
                &ss,
                &ts,
                Color::Yellow,
            );
        })
        .expect("warmup draw");
    let rows_ptr = app.diff.split_rows().as_ptr();
    let highlights_ptr = app.diff.line_highlights.as_ptr();

    let started = Instant::now();
    for _ in 0..100 {
        terminal
            .draw(|frame| {
                super::render(
                    frame,
                    &mut app,
                    Rect::new(0, 0, width, height),
                    &ss,
                    &ts,
                    Color::Yellow,
                );
            })
            .expect("repeat draw");
    }
    eprintln!("20k-line split render ×100: {:?}", started.elapsed());
    assert_eq!(app.diff.split_rows().as_ptr(), rows_ptr);
    assert_eq!(app.diff.line_highlights.as_ptr(), highlights_ptr);
}

/// Screen column of `needle`. `str::find` yields a byte offset, and the pane
/// border is a 3-byte `│`, so byte offsets are not columns here.
fn col_of(line: &str, needle: &str) -> usize {
    let byte = line
        .find(needle)
        .unwrap_or_else(|| panic!("{needle:?} not in {line:?}"));
    line[..byte].chars().count()
}

/// The `n` leftmost columns of a rendered row, counted in characters.
fn left_columns(line: &str, n: usize) -> String {
    line.chars().take(n).collect()
}

/// Everything from column `n` rightwards, counted in characters.
fn right_columns(line: &str, n: usize) -> String {
    line.chars().skip(n).collect()
}

/// Render the diff pane on its own and return the screen as lines of text.
fn drawn(app: &mut App, width: u16, height: u16, scroll_x: usize) -> Vec<String> {
    app.diff.scroll_x = scroll_x;
    let mut terminal = Terminal::new(TestBackend::new(width, height)).expect("a terminal");
    let ss = two_face::syntax::extra_newlines();
    let ts = ThemeSet::load_defaults();
    terminal
        .draw(|frame| {
            super::render(
                frame,
                app,
                Rect::new(0, 0, width, height),
                &ss,
                &ts,
                Color::Yellow,
            );
        })
        .expect("draw");
    let buf = terminal.backend().buffer();
    (0..buf.area.height)
        .map(|y| {
            (0..buf.area.width)
                .map(|x| buf[(x, y)].symbol())
                .collect::<String>()
        })
        .collect()
}
/// The split view pairs a removed line with its added counterpart on one row,
/// so this fixture gives the two sides *different* numbers — with both at 42
/// the test could not tell an old column from a new one.
fn skewed_pair_hunk() -> DiffHunk {
    DiffHunk {
        header: "@@ -42 +77 @@".to_string(),
        lines: vec![
            DiffLine {
                kind: LineKind::Removed,
                content: "gone();".to_string(),
                old_lineno: Some(42),
                new_lineno: None,
            },
            DiffLine {
                kind: LineKind::Added,
                content: "fresh();".to_string(),
                old_lineno: None,
                new_lineno: Some(77),
            },
        ],
        file_path: Some("src/lib.rs".to_string()),
    }
}

mod file_view;
mod split;
mod unified;
mod wrap;
