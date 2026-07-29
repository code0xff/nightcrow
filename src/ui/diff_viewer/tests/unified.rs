use super::*;

#[test]
fn unified_gutter_numbers_each_line_on_the_side_it_exists_on() {
    let mut app = app_showing(trio_hunk(), DiffPaneView::Diff);

    let screen = drawn(&mut app, 60, 10, 0);
    let body: Vec<&String> = screen.iter().filter(|l| l.contains("();")).collect();

    let context = body
        .iter()
        .find(|l| l.contains("keep_me"))
        .expect("the context row");
    let removed = body
        .iter()
        .find(|l| l.contains("gone"))
        .expect("the removed row");
    let added = body
        .iter()
        .find(|l| l.contains("fresh"))
        .expect("the added row");

    assert!(
        context.contains("41") && context.matches("41").count() == 2,
        "a context line exists on both sides, so both columns carry 41: {context:?}"
    );
    assert!(
        removed.contains("42"),
        "a removed line keeps its old number: {removed:?}"
    );
    assert!(
        removed.matches("42").count() == 1,
        "a removed line has no new-side number: {removed:?}"
    );
    assert!(
        added.matches("42").count() == 1,
        "an added line has only a new-side number: {added:?}"
    );
}

/// The reason the gutter is a separate paragraph at all.
#[test]
fn unified_gutter_stays_put_when_the_body_scrolls_sideways() {
    let mut app = app_showing(trio_hunk(), DiffPaneView::Diff);

    let unscrolled = drawn(&mut app, 60, 10, 0);
    let scrolled = drawn(&mut app, 60, 10, 6);

    let numbers_before = unscrolled.iter().filter(|l| l.contains("41")).count();
    let numbers_after = scrolled.iter().filter(|l| l.contains("41")).count();
    assert_eq!(
        numbers_before, numbers_after,
        "the gutter must survive horizontal scroll, got:\n{scrolled:#?}"
    );
    assert!(
        scrolled.iter().any(|l| !l.contains("keep_me()")),
        "the body should actually have scrolled, got:\n{scrolled:#?}"
    );
}

#[test]
fn a_hunk_header_reserves_the_same_gutter_width_as_the_body() {
    let mut app = app_showing(trio_hunk(), DiffPaneView::Diff);

    let screen = drawn(&mut app, 60, 10, 0);
    let header = screen
        .iter()
        .find(|l| l.contains("@@"))
        .expect("the hunk header row");
    let body = screen
        .iter()
        .find(|l| l.contains("keep_me"))
        .expect("the context row");

    // The body carries a `+`/`-`/space kind marker that the header does not, so
    // aligned means "one column apart", not "equal". Asserting the relationship
    // rather than a literal column keeps this from pinning the gutter's width.
    assert_eq!(
        col_of(header, "@@") + 1,
        col_of(body, "keep_me"),
        "header and body must start from the same gutter edge:\nheader {header:?}\nbody   {body:?}"
    );
    assert!(
        col_of(header, "@@") > 1,
        "the header must clear the reserved gutter rather than starting against \
         the border, got column {} in {header:?}",
        col_of(header, "@@")
    );
}

#[test]
fn an_empty_diff_reserves_no_gutter_for_its_placeholder() {
    let mut app = app_with_files(vec![]);
    app.mode = ViewMode::Status;

    let screen = drawn(&mut app, 60, 10, 0);
    let msg = screen
        .iter()
        .find(|l| l.contains("No changes"))
        .expect("the placeholder row");

    assert_eq!(
        col_of(msg, "No changes"),
        1,
        "with nothing to number the message sits against the border, not \
         indented under an empty gutter: {msg:?}"
    );
}
