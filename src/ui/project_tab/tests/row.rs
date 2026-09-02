//! The strip as a row across the top of the screen, the default placement.

use super::super::window::{ROW_MARKER_WIDTH, tab_segments, tab_texts};
use super::super::*;
use super::{crowded, paths, rendered, rendered_at};
use crate::config::TabStrip;
use ratatui::{Terminal, backend::TestBackend, layout::Rect, style::Color, text::Span};
use std::time::Duration;

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

    let segments = tab_segments(&many, &[], 0, 240, TabStrip::Top);

    assert_eq!(segments[9].0, " F10 p9 ");
    assert_eq!(segments[10].0, " p10 ");
}

#[test]
fn a_crowded_row_stays_within_its_width() {
    for active in [0usize, 5, 9] {
        let segments = tab_segments(&crowded(), &[], active, 80, TabStrip::Top);
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
    let segments = tab_segments(&crowded(), &[], 9, 80, TabStrip::Top);

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

    let segments = tab_segments(&projects, &attention, 9, 80, TabStrip::Top);

    assert!(segments[0].0.contains('•'), "got: {}", segments[0].0);
    assert_eq!(Span::raw(&segments[0].0).width(), ROW_MARKER_WIDTH as usize);
}

#[test]
fn a_row_that_fits_shows_no_markers() {
    let segments = tab_segments(&paths(&["/w/api", "/w/web"]), &[], 0, 80, TabStrip::Top);

    assert_eq!(segments.len(), 2, "no marker when everything fits");
}

#[test]
fn only_the_active_tab_is_accented() {
    let repo_paths = paths(&["/w/api", "/w/web"]);
    let mut terminal = Terminal::new(TestBackend::new(120, 1)).unwrap();
    terminal
        .draw(|frame| {
            frame.render_widget(
                render(
                    &repo_paths,
                    &[],
                    1,
                    frame.area(),
                    Color::Yellow,
                    true,
                    TabStrip::Top,
                ),
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
                        TabStrip::Top,
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

    assert!(phases[0].0.contains("F2•web"));
    assert_eq!(phases[0].0, phases[1].0, "blink must not move the row");
    assert_eq!(phases[0].1, Some(Color::Yellow));
    assert_eq!(phases[1].1, Some(Color::DarkGray));

    let plain = tab_texts(&repo_paths, &[false, false]);
    let unread = tab_texts(&repo_paths, &[false, true]);
    assert_eq!(Span::raw(&plain[1]).width(), Span::raw(&unread[1]).width());
}

#[test]
fn a_dot_in_a_project_name_is_not_treated_as_attention() {
    let repo_paths = paths(&["/w/api•server"]);
    let mut terminal = Terminal::new(TestBackend::new(120, 1)).unwrap();
    terminal
        .draw(|frame| {
            frame.render_widget(
                render(
                    &repo_paths,
                    &[false],
                    1,
                    frame.area(),
                    Color::Yellow,
                    true,
                    TabStrip::Top,
                ),
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
fn attention_blink_alternates_every_second() {
    assert!(blink_is_bright(Duration::ZERO));
    assert!(blink_is_bright(Duration::from_millis(999)));
    assert!(!blink_is_bright(Duration::from_secs(1)));
    assert!(!blink_is_bright(Duration::from_millis(1_999)));
    assert!(blink_is_bright(Duration::from_secs(2)));
}

#[test]
fn attention_dot_is_inside_its_project_hit_box() {
    let repo_paths = paths(&["/w/api", "/w/web"]);
    let area = Rect::new(0, 0, 120, 1);
    let text = tab_segments(&repo_paths, &[false, true], 0, area.width, TabStrip::Top)
        .into_iter()
        .map(|(text, _)| text)
        .collect::<String>();
    let dot = text.find('•').expect("attention dot rendered") as u16;

    assert_eq!(
        tab_at(&repo_paths, &[false, true], 0, area, dot, 0, TabStrip::Top),
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

    assert_eq!(
        tab_at(&repo_paths, &[], 0, area, 0, 0, TabStrip::Top),
        Some(0)
    );
    assert_eq!(
        tab_at(&repo_paths, &[], 0, area, web_x, 0, TabStrip::Top),
        Some(1)
    );
}

#[test]
fn tab_at_is_none_off_the_row_and_past_the_last_tab() {
    let repo_paths = paths(&["/w/api"]);
    let area = Rect::new(0, 0, 120, 1);

    assert_eq!(
        tab_at(&repo_paths, &[], 0, area, 0, 1, TabStrip::Top),
        None,
        "wrong row"
    );
    assert_eq!(
        tab_at(&repo_paths, &[], 0, area, 100, 0, TabStrip::Top),
        None,
        "past last tab"
    );

    // A layout too short to give the row any cells must not report hits:
    // whatever is drawn at that y belongs to another row.
    let collapsed = Rect::new(0, 0, 120, 0);
    assert_eq!(
        tab_at(&repo_paths, &[], 0, collapsed, 0, 0, TabStrip::Top),
        None
    );
}

#[test]
fn a_row_too_narrow_for_a_marker_still_shows_the_active_tab() {
    // Exactly the active tab's width: a marker beside it would push it off.
    let active_width = Span::raw(" F6 project-name-5 ").width() as u16;

    let segments = tab_segments(&crowded(), &[], 5, active_width, TabStrip::Top);

    assert_eq!(segments.len(), 1, "got: {segments:?}");
    assert_eq!(segments[0].1, 5);
}
