use super::*;

/// Regression: the file view already had a gutter, but it shared the body's
/// paragraph, so scrolling sideways carried the numbers off the left edge.
#[test]
fn file_view_line_numbers_stay_put_when_the_body_scrolls_sideways() {
    let mut app = app_with_files(vec!["src/lib.rs"]);
    app.git.view.mode = ViewMode::Status;
    app.git.view.diff.view = DiffPaneView::File;
    app.git.view.diff.file_view.key =
        Some(crate::app::FileViewKey::Status("src/lib.rs".to_string()));
    app.git.view.diff.file_view.content =
        "fn first() { let a_long_identifier = 1; }\nfn second() {}\n".to_string();

    let unscrolled = drawn_file_view(&mut app, 60, 8, 0);
    let scrolled = drawn_file_view(&mut app, 60, 8, 12);

    for screen in [&unscrolled, &scrolled] {
        assert!(
            screen.iter().any(|l| l.contains('1')) && screen.iter().any(|l| l.contains('2')),
            "both line numbers must be on screen, got:\n{screen:#?}"
        );
    }
    assert_ne!(
        unscrolled, scrolled,
        "the body should actually have scrolled"
    );
    assert_eq!(
        unscrolled
            .iter()
            .filter_map(|l| l.find(" 1 ").map(|_| ()))
            .count(),
        scrolled
            .iter()
            .filter_map(|l| l.find(" 1 ").map(|_| ()))
            .count(),
        "the numbered gutter column must survive the scroll:\n{scrolled:#?}"
    );
}

/// The file view reads its own horizontal offset, not `diff.scroll_x`.
fn drawn_file_view(app: &mut App, width: u16, height: u16, scroll_x: usize) -> Vec<String> {
    app.git.view.diff.file_view.scroll_x = scroll_x;
    drawn(app, width, height, 0)
}
