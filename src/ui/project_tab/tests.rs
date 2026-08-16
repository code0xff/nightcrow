use super::*;
use ratatui::{Terminal, backend::TestBackend, layout::Rect, style::Color, text::Span};
use std::time::Duration;

fn paths(v: &[&str]) -> Vec<String> {
    v.iter().map(|s| s.to_string()).collect()
}

fn rendered_at(repo_paths: &[String], active: usize, width: u16) -> String {
    let attention = vec![false; repo_paths.len()];
    let mut terminal = Terminal::new(TestBackend::new(width, 1)).unwrap();
    terminal
        .draw(|frame| {
            frame.render_widget(
                render(
                    repo_paths,
                    &attention,
                    active,
                    frame.area(),
                    Color::Yellow,
                    true,
                ),
                frame.area(),
            );
        })
        .unwrap();
    let buf = terminal.backend().buffer();
    (0..buf.area.width)
        .map(|x| buf[(x, 0)].symbol())
        .collect::<String>()
}

fn rendered(repo_paths: &[String], active: usize) -> String {
    rendered_at(repo_paths, active, 120)
}

#[test]
fn tab_label_uses_the_final_path_component() {
    assert_eq!(tab_label("/home/u/work/api"), "api");
    assert_eq!(tab_label("/home/u/work/api/"), "api");
}

#[test]
fn tab_label_handles_root_and_empty_paths() {
    // A blank tab would look unclickable, so both degenerate cases still
    // produce a visible name.
    assert_eq!(tab_label("/"), "/");
    assert_eq!(tab_label(""), "?");
}

#[test]
fn tab_label_truncates_a_long_name_with_an_ellipsis() {
    let label = tab_label("/w/a-very-long-project-name-here");

    assert_eq!(label.chars().count(), TAB_TITLE_MAX_CHARS);
    assert!(label.ends_with('…'), "got: {label}");
}

#[test]
fn every_tab_carries_its_f_key_legend() {
    let text = rendered(&paths(&["/w/api", "/w/web"]), 0);

    assert!(text.contains("F1 api"), "got: {text}");
    assert!(text.contains("F2 web"), "got: {text}");
}

#[test]
fn a_project_past_the_tenth_carries_no_key_legend() {
    // Only ten F-keys exist, so an eleventh tab must not imply one.
    let many: Vec<String> = (0..11).map(|i| format!("/w/p{i}")).collect();

    let segments = tab_segments(&many, &[], 0, 240);

    assert_eq!(segments[9].0, " F10 p9 ");
    assert_eq!(segments[10].0, " p10 ");
}

/// Ten tabs whose names are long enough that the row cannot hold them all
/// at 80 columns — the case a plain `Paragraph` would silently clip.
fn crowded() -> Vec<String> {
    (0..10).map(|i| format!("/w/project-name-{i}")).collect()
}

#[test]
fn a_crowded_row_stays_within_its_width() {
    for active in [0usize, 5, 9] {
        let segments = tab_segments(&crowded(), &[], active, 80);
        let total: u16 = segments
            .iter()
            .map(|(t, _)| Span::raw(t).width() as u16)
            .sum();

        assert!(
            total <= 80,
            "active={active} overflowed the row: {total} cells"
        );
    }
}

#[test]
fn the_active_tab_is_always_visible_however_crowded() {
    // Clipping the tail would hide the active tab — and with it the only
    // indication of which project the screen belongs to.
    for active in [0usize, 4, 9] {
        let text = rendered_at(&crowded(), active, 80);
        let expected = format!("F{} ", active + 1);

        assert!(
            text.contains(&expected),
            "active tab {active} must be on screen, got: {text}"
        );
    }
}

#[test]
fn hidden_tabs_are_reported_by_overflow_markers() {
    // Scrolled to the far end, everything before it is behind one marker.
    let segments = tab_segments(&crowded(), &[], 9, 80);

    let (marker, target) = &segments[0];
    assert!(marker.starts_with(" +"), "got: {marker}");
    // The marker selects the nearest hidden project, so the overflow is
    // reachable by pointer and not only by F-key.
    assert_eq!(
        *target,
        marker
            .trim()
            .trim_start_matches('+')
            .parse::<usize>()
            .unwrap()
            - 1
    );
}

#[test]
fn an_overflow_marker_carries_attention_from_any_hidden_project() {
    let projects = crowded();
    let mut attention = vec![false; projects.len()];
    attention[0] = true;

    let segments = tab_segments(&projects, &attention, 9, 80);

    assert!(segments[0].0.contains('•'), "got: {}", segments[0].0);
    assert_eq!(Span::raw(&segments[0].0).width(), MARKER_WIDTH as usize);
}

#[test]
fn a_row_that_fits_shows_no_markers() {
    let segments = tab_segments(&paths(&["/w/api", "/w/web"]), &[], 0, 80);

    assert_eq!(segments.len(), 2, "no marker when everything fits");
}

#[test]
fn only_the_active_tab_is_accented() {
    let repo_paths = paths(&["/w/api", "/w/web"]);
    let mut terminal = Terminal::new(TestBackend::new(120, 1)).unwrap();
    terminal
        .draw(|frame| {
            frame.render_widget(
                render(&repo_paths, &[], 1, frame.area(), Color::Yellow, true),
                frame.area(),
            );
        })
        .unwrap();

    let buf = terminal.backend().buffer();
    let accented: String = (0..buf.area.width)
        .filter(|&x| buf[(x, 0)].style().bg == Some(Color::Yellow))
        .map(|x| buf[(x, 0)].symbol())
        .collect();

    assert!(accented.contains("web"), "got: {accented}");
    assert!(!accented.contains("api"), "got: {accented}");
}

#[test]
fn unread_attention_is_one_fixed_width_blinking_dot() {
    let repo_paths = paths(&["/w/api", "/w/web"]);
    let mut phases = Vec::new();
    for bright in [true, false] {
        let mut terminal = Terminal::new(TestBackend::new(120, 1)).unwrap();
        terminal
            .draw(|frame| {
                frame.render_widget(
                    render(
                        &repo_paths,
                        &[false, true],
                        0,
                        frame.area(),
                        Color::Yellow,
                        bright,
                    ),
                    frame.area(),
                );
            })
            .unwrap();
        let buf = terminal.backend().buffer();
        let text = (0..buf.area.width)
            .map(|x| buf[(x, 0)].symbol())
            .collect::<String>();
        let dot = (0..buf.area.width)
            .find(|&x| buf[(x, 0)].symbol() == "•")
            .expect("attention dot rendered");
        phases.push((text, buf[(dot, 0)].style().fg));
    }

    assert!(phases[0].0.contains("F2 • web"));
    assert_eq!(phases[0].0, phases[1].0, "blink must not move the row");
    assert_eq!(phases[0].1, Some(Color::Yellow));
    assert_eq!(phases[1].1, Some(Color::DarkGray));
}

#[test]
fn a_dot_in_a_project_name_is_not_treated_as_attention() {
    let repo_paths = paths(&["/w/api•server"]);
    let mut terminal = Terminal::new(TestBackend::new(120, 1)).unwrap();
    terminal
        .draw(|frame| {
            frame.render_widget(
                render(&repo_paths, &[false], 1, frame.area(), Color::Yellow, true),
                frame.area(),
            );
        })
        .unwrap();

    let buf = terminal.backend().buffer();
    let name_dot = (0..buf.area.width)
        .find(|&x| buf[(x, 0)].symbol() == "•")
        .expect("project name dot rendered");
    assert_eq!(buf[(name_dot, 0)].style().fg, Some(Color::Gray));
}

#[test]
fn attention_blink_alternates_every_half_second() {
    assert!(blink_is_bright(Duration::ZERO));
    assert!(!blink_is_bright(Duration::from_millis(500)));
    assert!(blink_is_bright(Duration::from_secs(1)));
}

#[test]
fn attention_dot_is_inside_its_project_hit_box() {
    let repo_paths = paths(&["/w/api", "/w/web"]);
    let area = Rect::new(0, 0, 120, 1);
    let text = tab_segments(&repo_paths, &[false, true], 0, area.width)
        .into_iter()
        .map(|(text, _)| text)
        .collect::<String>();
    let dot = text.find('•').expect("attention dot rendered") as u16;

    assert_eq!(
        tab_at(&repo_paths, &[false, true], 0, area, dot, 0),
        Some(1)
    );
}

#[test]
fn tab_at_maps_a_click_to_the_tab_under_it() {
    let repo_paths = paths(&["/w/api", "/w/web"]);
    let area = Rect::new(0, 0, 120, 1);
    // Walk the rendered row rather than trusting the builder alone: the
    // hit box must match the glyphs actually on screen.
    let text = rendered(&repo_paths, 0);
    let web_x = text.find("F2 web").expect("second tab rendered") as u16;

    assert_eq!(tab_at(&repo_paths, &[], 0, area, 0, 0), Some(0));
    assert_eq!(tab_at(&repo_paths, &[], 0, area, web_x, 0), Some(1));
}

#[test]
fn tab_at_is_none_off_the_row_and_past_the_last_tab() {
    let repo_paths = paths(&["/w/api"]);
    let area = Rect::new(0, 0, 120, 1);

    assert_eq!(tab_at(&repo_paths, &[], 0, area, 0, 1), None, "wrong row");
    assert_eq!(
        tab_at(&repo_paths, &[], 0, area, 100, 0),
        None,
        "past last tab"
    );

    // A layout too short to give the row any cells must not report hits:
    // whatever is drawn at that y belongs to another row.
    let collapsed = Rect::new(0, 0, 120, 0);
    assert_eq!(tab_at(&repo_paths, &[], 0, collapsed, 0, 0), None);
}
