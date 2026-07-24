use crate::app::tests::app_with_files;
use crate::ui::terminal_tab::layout::{split_pane_areas, terminal_layout, truncate_tab_title};
use crate::ui::terminal_tab::render;
use crate::ui::terminal_tab::screen::screen_cursor_position;
use ratatui::{Terminal, backend::TestBackend, layout::{Position, Rect}, style::Color};

#[test]
fn maps_screen_cursor_to_render_area() {
    let mut emulator = crate::runtime::emulator::PaneEmulator::new(3, 10, 0);
    emulator.process(b"\x1b[2;4H");

    let position = screen_cursor_position(&emulator.view(), Rect::new(20, 10, 10, 3)).unwrap();

    assert_eq!(position, Position::new(23, 11));
}

#[test]
fn short_title_passes_through_untouched() {
    assert_eq!(truncate_tab_title("claude", 24), "claude");
}

#[test]
fn long_title_is_cut_with_ellipsis_within_budget() {
    let truncated = truncate_tab_title("claude-code: very-long-project-name", 24);
    assert_eq!(truncated.chars().count(), 24);
    assert!(truncated.ends_with('…'));
    assert!(truncated.starts_with("claude-code"));
}

#[test]
fn title_exactly_at_budget_is_not_truncated() {
    let s: String = "a".repeat(24);
    assert_eq!(truncate_tab_title(&s, 24), s);
}

#[test]
fn keeps_cursor_visible_when_terminal_requests_hide() {
    let mut emulator = crate::runtime::emulator::PaneEmulator::new(3, 10, 0);
    emulator.process(b"\x1b[?25l\x1b[2;4H");

    let position = screen_cursor_position(&emulator.view(), Rect::new(20, 10, 10, 3)).unwrap();

    assert_eq!(position, Position::new(23, 11));
}

#[test]
fn content_spans_full_width_without_side_borders() {
    // The terminal content must reach both pane edges so copied output never
    // includes a `│`. Side borders would inset x by 1 and shrink width by 2.
    let area = Rect::new(0, 0, 40, 10);
    let (_, content) = terminal_layout(area).unwrap();

    assert_eq!(content.x, area.x);
    assert_eq!(content.width, area.width);
}

#[test]
fn render_does_not_resize_terminal_state() {
    let mut app = app_with_files(vec!["a.rs"]);
    app.terminal.size = (3, 10);
    let mut terminal = Terminal::new(TestBackend::new(40, 10)).unwrap();

    terminal
        .draw(|frame| {
            render(frame, &app, frame.area(), Color::Yellow);
        })
        .unwrap();

    assert_eq!(app.terminal.size, (3, 10));
}

#[test]
fn split_pane_areas_single_pane_fills_area() {
    let area = Rect::new(0, 0, 80, 24);
    assert_eq!(split_pane_areas(area, 1), vec![area]);
}

#[test]
fn split_pane_areas_two_panes_side_by_side_when_wide() {
    let area = Rect::new(0, 0, 80, 24);
    let cells = split_pane_areas(area, 2);
    assert_eq!(cells.len(), 2);
    assert_eq!(cells[0].y, cells[1].y);
    assert_ne!(cells[0].x, cells[1].x);
}

#[test]
fn split_pane_areas_two_panes_stacked_when_narrow() {
    let area = Rect::new(0, 0, 30, 24);
    let cells = split_pane_areas(area, 2);
    assert_eq!(cells.len(), 2);
    assert_eq!(cells[0].x, cells[1].x);
    assert_ne!(cells[0].y, cells[1].y);
}

#[test]
fn split_pane_areas_three_panes_two_over_one() {
    let area = Rect::new(0, 0, 80, 24);
    let cells = split_pane_areas(area, 3);
    assert_eq!(cells.len(), 3);
    // First row: two side-by-side cells.
    assert_eq!(cells[0].y, cells[1].y);
    // Second row: one full-width cell below.
    assert!(cells[2].y > cells[0].y);
}

#[test]
fn split_pane_areas_four_panes_is_2x2() {
    let area = Rect::new(0, 0, 80, 24);
    let cells = split_pane_areas(area, 4);
    assert_eq!(cells.len(), 4);
    let rows: std::collections::BTreeSet<u16> = cells.iter().map(|r| r.y).collect();
    assert_eq!(rows.len(), 2);
}

#[test]
fn split_pane_areas_seven_panes_four_then_three() {
    let area = Rect::new(0, 0, 100, 30);
    let cells = split_pane_areas(area, 7);
    assert_eq!(cells.len(), 7);
    let top_row_y = cells[0].y;
    let top_row_count = cells.iter().filter(|r| r.y == top_row_y).count();
    assert_eq!(top_row_count, 4);
}

#[test]
fn split_pane_areas_eight_panes_is_2x4() {
    let area = Rect::new(0, 0, 100, 30);
    let cells = split_pane_areas(area, 8);
    assert_eq!(cells.len(), 8);
    let rows: std::collections::BTreeSet<u16> = cells.iter().map(|r| r.y).collect();
    assert_eq!(rows.len(), 2);
    let top_row_y = cells[0].y;
    let top_row_count = cells.iter().filter(|r| r.y == top_row_y).count();
    assert_eq!(top_row_count, 4);
}

#[test]
fn split_pane_areas_never_produces_zero_size_cells_when_area_fits() {
    for count in 1..=8 {
        let area = Rect::new(0, 0, 40, 20);
        let cells = split_pane_areas(area, count);
        assert_eq!(cells.len(), count, "count={count}");
        for cell in cells {
            assert!(
                cell.width > 0 && cell.height > 0,
                "count={count} cell={cell:?}"
            );
        }
    }
}

#[test]
fn split_pane_areas_empty_for_zero_count() {
    assert!(split_pane_areas(Rect::new(0, 0, 80, 24), 0).is_empty());
}
