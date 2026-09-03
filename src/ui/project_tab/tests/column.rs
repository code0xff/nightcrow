//! The strip as a column (`[layout] tabs = "left"`): one tab per row, the
//! same labels and overflow markers as the row, padded to the strip's width.

use super::super::window::tab_segments;
use super::super::{STRIP_WIDTH, render, tab_at};
use super::{crowded, paths};
use crate::config::TabStrip;
use ratatui::{Terminal, backend::TestBackend, layout::Rect, style::Color};

/// The strip drawn into a column `height` rows tall, one string per row.
fn rendered_column(repo_paths: &[String], active: usize, height: u16) -> Vec<String> {
    let attention = vec![false; repo_paths.len()];
    let mut terminal = Terminal::new(TestBackend::new(STRIP_WIDTH, height)).unwrap();
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
                    TabStrip::Left,
                ),
                frame.area(),
            );
        })
        .unwrap();
    let buf = terminal.backend().buffer();
    (0..height)
        .map(|y| (0..STRIP_WIDTH).map(|x| buf[(x, y)].symbol()).collect())
        .collect()
}

#[test]
fn each_tab_takes_one_row_with_its_legend() {
    let rows = rendered_column(&paths(&["/w/api", "/w/web"]), 0, 4);

    assert!(rows[0].starts_with(" F1 api"), "got: {:?}", rows[0]);
    assert!(rows[1].starts_with(" F2 web"), "got: {:?}", rows[1]);
    assert_eq!(rows[2].trim(), "", "no third project, so a blank row");
}

#[test]
fn the_active_row_is_accented_across_the_whole_strip() {
    // The row is the click box, so the highlight must reach its edge: a label
    // that stopped short would leave cells that look like the tab and are not.
    let repo_paths = paths(&["/w/api", "/w/web"]);
    let mut terminal = Terminal::new(TestBackend::new(STRIP_WIDTH, 3)).unwrap();
    terminal
        .draw(|frame| {
            frame.render_widget(
                render(
                    &repo_paths,
                    &[false, false],
                    1,
                    frame.area(),
                    Color::Yellow,
                    true,
                    TabStrip::Left,
                ),
                frame.area(),
            );
        })
        .unwrap();
    let buf = terminal.backend().buffer();

    for x in 0..STRIP_WIDTH {
        assert_eq!(buf[(x, 1)].bg, Color::Yellow, "active row, cell {x}");
        assert_ne!(buf[(x, 0)].bg, Color::Yellow, "inactive row, cell {x}");
    }
}

#[test]
fn a_long_name_is_cut_by_the_label_rule_not_by_the_strip() {
    // 14 characters and an ellipsis fit the 20-cell strip beside the widest
    // legend, so the width never has to cut what the rule already did.
    let rows = rendered_column(&paths(&["/w/a-very-long-project-name"]), 0, 1);

    assert!(rows[0].contains("a-very-long-p…"), "got: {:?}", rows[0]);
    assert_eq!(rows[0].chars().count(), STRIP_WIDTH as usize);
}

#[test]
fn a_crowded_column_stays_within_its_height() {
    for active in [0usize, 5, 9] {
        let segments = tab_segments(&crowded(), &[], active, 5, TabStrip::Left);

        assert!(
            segments.len() <= 5,
            "active={active} overflowed the column: {} rows",
            segments.len()
        );
        assert!(
            segments.iter().any(|(_, index)| *index == active),
            "active={active} was scrolled out of its own strip"
        );
    }
}

#[test]
fn hidden_tabs_are_reported_by_marker_rows() {
    // Ten tabs in five rows around the last one: what is above folds into a
    // single `+N` row, and there is nothing below to fold.
    let segments = tab_segments(&crowded(), &[], 9, 5, TabStrip::Left);

    assert!(segments[0].0.starts_with(" +"), "got: {:?}", segments[0].0);
    assert_eq!(segments[0].0.trim(), "+6");
    assert_eq!(segments.last().unwrap().1, 9);
    assert!(!segments.last().unwrap().0.starts_with(" +"));
}

#[test]
fn tab_at_maps_a_row_to_the_tab_drawn_on_it() {
    let repo_paths = paths(&["/w/api", "/w/web"]);
    let area = Rect::new(0, 0, STRIP_WIDTH, 10);

    assert_eq!(
        tab_at(&repo_paths, &[], 0, area, 3, 0, TabStrip::Left),
        Some(0)
    );
    assert_eq!(
        tab_at(&repo_paths, &[], 0, area, 3, 1, TabStrip::Left),
        Some(1)
    );
    // The whole row is the tab, out to the strip's last cell.
    assert_eq!(
        tab_at(
            &repo_paths,
            &[],
            0,
            area,
            STRIP_WIDTH - 1,
            1,
            TabStrip::Left
        ),
        Some(1)
    );
}

#[test]
fn tab_at_is_none_beside_the_strip_and_below_the_last_tab() {
    let repo_paths = paths(&["/w/api", "/w/web"]);
    let area = Rect::new(0, 0, STRIP_WIDTH, 10);

    assert_eq!(
        tab_at(&repo_paths, &[], 0, area, STRIP_WIDTH, 0, TabStrip::Left),
        None,
        "first body column"
    );
    assert_eq!(
        tab_at(&repo_paths, &[], 0, area, 3, 2, TabStrip::Left),
        None,
        "blank row under the tabs"
    );

    // A strip given no rows must not report hits: whatever is at that cell
    // belongs to something else.
    let collapsed = Rect::new(0, 0, STRIP_WIDTH, 0);
    assert_eq!(
        tab_at(&repo_paths, &[], 0, collapsed, 3, 0, TabStrip::Left),
        None
    );
}

#[test]
fn a_marker_row_selects_the_nearest_hidden_project() {
    // Same rule as the row's `+N` cells: the overflow stays reachable by
    // pointer, one project at a time from the edge.
    let area = Rect::new(0, 0, STRIP_WIDTH, 5);

    let above = tab_at(&crowded(), &[], 9, area, 3, 0, TabStrip::Left);

    let segments = tab_segments(&crowded(), &[], 9, 5, TabStrip::Left);
    assert_eq!(above, Some(segments[0].1));
    assert_eq!(above, Some(5), "the tab just above the visible run");
}

#[test]
fn the_strip_is_offset_by_its_own_origin() {
    // The column does not start at the screen's corner in every layout, and
    // the hit test must read rows relative to where it was drawn.
    let repo_paths = paths(&["/w/api", "/w/web"]);
    let area = Rect::new(4, 2, STRIP_WIDTH, 10);

    assert_eq!(
        tab_at(&repo_paths, &[], 0, area, 5, 3, TabStrip::Left),
        Some(1)
    );
    assert_eq!(
        tab_at(&repo_paths, &[], 0, area, 5, 1, TabStrip::Left),
        None
    );
    assert_eq!(
        tab_at(&repo_paths, &[], 0, area, 2, 3, TabStrip::Left),
        None
    );
}

#[test]
fn a_one_row_strip_shows_the_active_tab_and_no_marker() {
    // One row for ten tabs, the active one in the middle: there is no room for
    // a marker beside it, and the marker must not take the row from the one
    // segment the strip exists to show. The row is the tab, and nothing below
    // the strip is.
    let area = Rect::new(0, 0, STRIP_WIDTH, 1);

    let segments = tab_segments(&crowded(), &[], 5, 1, TabStrip::Left);
    assert_eq!(segments.len(), 1, "got: {segments:?}");
    assert_eq!(segments[0].1, 5);

    assert_eq!(
        tab_at(&crowded(), &[], 5, area, 3, 0, TabStrip::Left),
        Some(5)
    );
    assert_eq!(tab_at(&crowded(), &[], 5, area, 3, 1, TabStrip::Left), None);
}

#[test]
fn a_wide_character_name_is_cut_by_cells_and_never_overflows_the_strip() {
    // Fourteen CJK characters are twenty-eight cells: the label rule, which
    // counts characters, lets them through, so the row has to cut by cells —
    // and land exactly on the strip's width without splitting a glyph.
    let rows = rendered_column(&paths(&["/w/프로젝트이름이아주아주긴저장소"]), 0, 1);

    // The buffer yields one symbol per cell, a wide glyph then a blank for its
    // second cell: sixteen cells hold eight glyphs after the legend, so the
    // eighth glyph is the last one drawn and the ninth never is.
    let row = &rows[0];
    assert!(row.starts_with(" F1 프"), "got: {row:?}");
    assert!(row.contains('아') && !row.contains('주'), "got: {row:?}");
}

#[test]
fn a_strip_too_narrow_for_the_attention_glyph_draws_the_tab_plain() {
    // A terminal narrower than the strip hands the column fewer cells than
    // ` F2•` needs, and the glyph is cut before the styling sees the row. The
    // tab is drawn without its marker; it is not a reason to stop drawing.
    let repo_paths = paths(&["/w/api", "/w/web"]);
    let mut terminal = Terminal::new(TestBackend::new(3, 2)).unwrap();
    terminal
        .draw(|frame| {
            frame.render_widget(
                render(
                    &repo_paths,
                    &[false, true],
                    0,
                    frame.area(),
                    Color::Yellow,
                    true,
                    TabStrip::Left,
                ),
                frame.area(),
            );
        })
        .unwrap();
    let buf = terminal.backend().buffer();
    let row: String = (0..3).map(|x| buf[(x, 1)].symbol()).collect();

    assert_eq!(row, " F2");
}
