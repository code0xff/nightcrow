use crate::ui::diff_pane::HIGHLIGHT_LINE_CAP;
use crate::ui::file_view::FileViewState;
use syntect::highlighting::ThemeSet;
use syntect::parsing::{SyntaxReference, SyntaxSet};

fn content_of(count: usize) -> String {
    (1..=count)
        .map(|n| format!("line {n}"))
        .collect::<Vec<_>>()
        .join("\n")
}

fn terminated_content_of(count: usize) -> String {
    let mut content = content_of(count);
    content.push('\n');
    content
}

fn full_rebuild(
    content: &str,
    ss: &SyntaxSet,
    ts: &ThemeSet,
    syntax: &SyntaxReference,
) -> Vec<Vec<crate::ui::diff_pane::HighlightSegment>> {
    let mut fresh = FileViewState::default();
    fresh.set_content(content.to_string());
    fresh.ensure_highlight_cache(ss, ts, syntax);
    fresh.line_highlights
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

#[test]
fn appending_keeps_the_cached_prefix_and_matches_a_full_rebuild() {
    let ss = two_face::syntax::extra_newlines();
    let ts = ThemeSet::load_defaults();
    let syntax = ss.find_syntax_by_extension("rs").unwrap();
    let base = "/* open\nstill comment\n*/\nfn first() {}\n".to_string();

    let mut view = FileViewState::default();
    view.set_content(base.clone());
    view.ensure_highlight_cache(&ss, &ts, syntax);
    let cached = view.line_highlights.clone();

    let appended = format!("{base}fn second() {{ let x = 1; }}\n");
    view.set_content(appended.clone());
    view.ensure_highlight_cache(&ss, &ts, syntax);

    assert_eq!(
        view.line_highlights,
        full_rebuild(&appended, &ss, &ts, syntax)
    );
    assert_eq!(view.line_highlights[..cached.len()], cached[..]);
}

#[test]
fn an_append_across_the_cap_matches_a_full_rebuild() {
    let ss = two_face::syntax::extra_newlines();
    let ts = ThemeSet::load_defaults();
    let syntax = ss.find_syntax_plain_text();

    let base = terminated_content_of(HIGHLIGHT_LINE_CAP - 4);
    let mut view = FileViewState::default();
    view.set_content(base.clone());
    view.ensure_highlight_cache(&ss, &ts, syntax);

    let appended = format!("{base}{}", content_of(20));
    view.set_content(appended.clone());
    view.ensure_highlight_cache(&ss, &ts, syntax);

    assert_eq!(view.line_highlights.len(), HIGHLIGHT_LINE_CAP);
    assert_eq!(
        view.line_highlights,
        full_rebuild(&appended, &ss, &ts, syntax)
    );
}

#[test]
fn a_file_append_past_a_full_cap_matches_a_full_rebuild() {
    let ss = two_face::syntax::extra_newlines();
    let ts = ThemeSet::load_defaults();
    let syntax = ss.find_syntax_plain_text();

    let base = terminated_content_of(HIGHLIGHT_LINE_CAP);
    let mut view = FileViewState::default();
    view.set_content(base.clone());
    view.ensure_highlight_cache(&ss, &ts, syntax);

    let appended = format!("{base}{}", content_of(20));
    view.set_content(appended.clone());
    view.ensure_highlight_cache(&ss, &ts, syntax);

    assert_eq!(view.line_highlights.len(), HIGHLIGHT_LINE_CAP);
    assert_eq!(
        view.line_highlights,
        full_rebuild(&appended, &ss, &ts, syntax)
    );
}

#[test]
fn a_middle_edit_matches_a_full_rebuild() {
    let ss = two_face::syntax::extra_newlines();
    let ts = ThemeSet::load_defaults();
    let syntax = ss.find_syntax_plain_text();

    let base = terminated_content_of(40);
    let mut view = FileViewState::default();
    view.set_content(base.clone());
    view.ensure_highlight_cache(&ss, &ts, syntax);

    let edited = base.replace("line 20\n", "line twenty\n");
    view.set_content(edited.clone());
    view.ensure_highlight_cache(&ss, &ts, syntax);

    assert_eq!(
        view.line_highlights,
        full_rebuild(&edited, &ss, &ts, syntax)
    );
}

#[test]
fn extending_an_unterminated_last_line_matches_a_full_rebuild() {
    let ss = two_face::syntax::extra_newlines();
    let ts = ThemeSet::load_defaults();
    let syntax = ss.find_syntax_plain_text();

    let mut view = FileViewState::default();
    view.set_content("fn first() {}".to_string());
    view.ensure_highlight_cache(&ss, &ts, syntax);

    let extended = "fn first() { let x = 1; }".to_string();
    view.set_content(extended.clone());
    view.ensure_highlight_cache(&ss, &ts, syntax);

    assert_eq!(
        view.line_highlights,
        full_rebuild(&extended, &ss, &ts, syntax)
    );
}

#[test]
fn appending_under_a_different_syntax_matches_a_full_rebuild() {
    let ss = two_face::syntax::extra_newlines();
    let ts = ThemeSet::load_defaults();

    let base = terminated_content_of(10);
    let mut view = FileViewState::default();
    view.set_content(base.clone());
    view.ensure_highlight_cache(&ss, &ts, ss.find_syntax_by_extension("rs").unwrap());

    let appended = format!("{base}{}", content_of(5));
    view.set_content(appended.clone());
    let plain = ss.find_syntax_plain_text();
    view.ensure_highlight_cache(&ss, &ts, plain);

    assert_eq!(
        view.line_highlights,
        full_rebuild(&appended, &ss, &ts, plain)
    );
}
