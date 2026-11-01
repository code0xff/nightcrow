//! Rendering at and past the syntax-highlight line cap.

use super::*;
use crate::ui::diff_pane::HIGHLIGHT_LINE_CAP;
use ratatui::buffer::Buffer;

fn render_buffer(app: &mut App, width: u16, height: u16) -> (Vec<String>, Buffer) {
    let mut terminal = Terminal::new(TestBackend::new(width, height)).expect("a terminal");
    let ss = two_face::syntax::extra_newlines();
    let ts = ThemeSet::load_defaults();
    terminal
        .draw(|frame| {
            super::super::render(
                frame,
                app,
                Rect::new(0, 0, width, height),
                &ss,
                &ts,
                Color::Yellow,
            );
        })
        .expect("draw");
    let buf = terminal.backend().buffer().clone();
    let screen = (0..buf.area.height)
        .map(|y| {
            (0..buf.area.width)
                .map(|x| buf[(x, y)].symbol())
                .collect::<String>()
        })
        .collect();
    (screen, buf)
}

fn added_hunk(count: usize) -> DiffHunk {
    DiffHunk {
        header: "@@ -0,0 +1 @@".to_string(),
        lines: (1..=count)
            .map(|n| DiffLine {
                kind: LineKind::Added,
                content: format!("added {n}"),
                old_lineno: None,
                new_lineno: Some(n as u32),
            })
            .collect(),
        file_path: None,
    }
}

fn row_of(screen: &[String], needle: &str) -> usize {
    screen
        .iter()
        .position(|line| line.contains(needle))
        .unwrap_or_else(|| panic!("{needle:?} not on screen:\n{screen:#?}"))
}

fn cell_at(screen: &[String], buf: &Buffer, row: usize, needle: &str) -> (Color, Color) {
    let x = col_of(&screen[row], needle) as u16;
    let cell = &buf[(x, row as u16)];
    (cell.fg, cell.bg)
}

#[test]
fn unified_renders_past_the_cap_with_the_added_background() {
    let mut app = app_showing(added_hunk(HIGHLIGHT_LINE_CAP + 1), DiffPaneView::Diff);
    // The flat row of the last body line: one hunk header plus its index.
    app.git.view.diff.scroll = HIGHLIGHT_LINE_CAP;
    let (screen, buf) = render_buffer(&mut app, 60, 8);

    let highlighted: usize = app.git.view.diff.line_highlights.iter().map(Vec::len).sum();
    assert_eq!(highlighted, HIGHLIGHT_LINE_CAP);

    let over = format!("added {}", HIGHLIGHT_LINE_CAP + 1);
    let row = row_of(&screen, &over);
    let (fg, bg) = cell_at(&screen, &buf, row, &over);
    assert_eq!(
        bg,
        Color::Rgb(0, 50, 0),
        "the raw tail keeps its added background"
    );
    assert_eq!(fg, Color::Reset, "the raw tail is not syntax-colored");

    let under = format!("added {HIGHLIGHT_LINE_CAP}");
    let row = row_of(&screen, &under);
    let (fg, _) = cell_at(&screen, &buf, row, &under);
    assert_ne!(fg, Color::Reset, "lines under the cap still use the cache");
}

#[test]
fn split_renders_past_the_cap_with_the_added_background() {
    let mut app = app_showing(added_hunk(HIGHLIGHT_LINE_CAP + 1), DiffPaneView::Split);
    // Last split row: one header plus one row per added line.
    app.git.view.diff.scroll = HIGHLIGHT_LINE_CAP + 1;
    let (screen, buf) = render_buffer(&mut app, 120, 8);
    assert!(
        screen.join("\n").contains("[split]"),
        "the split layout must actually be in use:\n{screen:#?}"
    );

    let over = format!("added {}", HIGHLIGHT_LINE_CAP + 1);
    let row = row_of(&screen, &over);
    let (fg, bg) = cell_at(&screen, &buf, row, &over);
    assert_eq!(
        bg,
        Color::Rgb(0, 50, 0),
        "the raw tail keeps its added background"
    );
    assert_eq!(fg, Color::Reset, "the raw tail is not syntax-colored");
}

#[test]
fn file_overlay_renders_past_the_cap() {
    let mut app = app_with_files(vec!["notes.txt"]);
    app.git.view.mode = ViewMode::Status;
    app.git.view.diff.view = DiffPaneView::File;
    app.git.view.diff.file_view.key =
        Some(crate::app::FileViewKey::Status("notes.txt".to_string()));
    let content = (1..=HIGHLIGHT_LINE_CAP + 1)
        .map(|n| format!("line {n}"))
        .collect::<Vec<_>>()
        .join("\n");
    app.git.view.diff.file_view.set_content(content);
    app.git.view.diff.file_view.scroll = HIGHLIGHT_LINE_CAP;

    let (screen, buf) = render_buffer(&mut app, 60, 8);
    assert_eq!(
        app.git.view.diff.file_view.line_highlights.len(),
        HIGHLIGHT_LINE_CAP
    );

    let over = format!("line {}", HIGHLIGHT_LINE_CAP + 1);
    let row = row_of(&screen, &over);
    let (fg, _) = cell_at(&screen, &buf, row, &over);
    assert_eq!(fg, Color::Reset, "the tail renders as plain text");
}
