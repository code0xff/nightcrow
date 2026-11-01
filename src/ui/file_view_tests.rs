use crate::ui::diff_pane::HIGHLIGHT_LINE_CAP;
use crate::ui::file_view::FileViewState;

fn content_of(count: usize) -> String {
    (1..=count)
        .map(|n| format!("line {n}"))
        .collect::<Vec<_>>()
        .join("\n")
}

#[test]
fn highlight_cache_stops_at_the_line_cap() {
    let ss = two_face::syntax::extra_newlines();
    let ts = syntect::highlighting::ThemeSet::load_defaults();
    let syntax = ss.find_syntax_plain_text();
    for count in [
        HIGHLIGHT_LINE_CAP - 1,
        HIGHLIGHT_LINE_CAP,
        HIGHLIGHT_LINE_CAP + 1,
    ] {
        let mut view = FileViewState::default();
        view.set_content(content_of(count));
        view.ensure_highlight_cache(&ss, &ts, syntax);
        assert_eq!(view.line_count(), count);
        assert_eq!(
            view.line_highlights.len(),
            count.min(HIGHLIGHT_LINE_CAP),
            "{count} lines"
        );
    }
}

#[test]
fn lower_cache_is_built_only_when_requested() {
    let mut view = FileViewState::default();
    view.set_content("Alpha\nBETA\n".to_string());
    assert!(view.lines_lower.is_empty());

    view.ensure_lower_cache();
    assert_eq!(view.lines_lower, vec!["alpha", "beta"]);
}

#[test]
fn line_text_matches_str_lines_semantics() {
    let mut view = FileViewState::default();
    view.set_content("a\r\nb\n\nc".to_string());

    assert_eq!(view.line_count(), 4);
    assert_eq!(view.line_text(0), Some("a"));
    assert_eq!(view.line_text(1), Some("b"));
    assert_eq!(view.line_text(2), Some(""));
    assert_eq!(view.line_text(3), Some("c"));
    assert_eq!(view.line_text(4), None);
}

#[test]
fn line_text_remains_available_past_the_highlight_cap() {
    let ss = two_face::syntax::extra_newlines();
    let ts = syntect::highlighting::ThemeSet::load_defaults();
    let syntax = ss.find_syntax_plain_text();
    let mut view = FileViewState::default();
    view.set_content(content_of(HIGHLIGHT_LINE_CAP + 3));
    view.ensure_highlight_cache(&ss, &ts, syntax);

    assert_eq!(view.line_highlights.len(), HIGHLIGHT_LINE_CAP);
    let expected = format!("line {}", HIGHLIGHT_LINE_CAP + 3);
    assert_eq!(
        view.line_text(HIGHLIGHT_LINE_CAP + 2),
        Some(expected.as_str())
    );
}
