use crate::app::tests::app_with_files;
use crate::app::Focus;
use crate::config::LayoutConfig;
use crate::runtime::terminal::TerminalFullscreen;
use crate::ui::{pane_at, terminal_content_areas, upper_panel_at};
use ratatui::layout::Rect;

#[test]
fn terminal_content_areas_hidden_when_other_pane_is_fullscreen() {
    let mut app = app_with_files(vec!["a.rs"]);
    app.toggle_diff_fullscreen();

    let areas =
        terminal_content_areas(&app, Rect::new(0, 0, 100, 40), &LayoutConfig::default());

    assert!(areas.is_empty());
}

#[test]
fn terminal_content_areas_uses_body_when_terminal_fullscreen() {
    let mut app = app_with_files(vec!["a.rs"]);
    app.terminal.panes.push(crate::app::PaneInfo {
        id: 1,
        title: "shell".to_string(),
    });
    let screen = Rect::new(0, 0, 100, 40);
    let layout = LayoutConfig::default();
    let areas = terminal_content_areas(&app, screen, &layout);
    assert!(!areas.is_empty());
}

#[test]
fn pane_at_resolves_the_pane_under_a_cell_and_misses_elsewhere() {
    let mut app = app_with_files(vec!["a.rs"]);
    app.terminal.panes.push(crate::app::PaneInfo {
        id: 1,
        title: "shell".to_string(),
    });
    app.terminal.panes.push(crate::app::PaneInfo {
        id: 2,
        title: "shell".to_string(),
    });
    let screen = Rect::new(0, 0, 100, 40);
    let layout = LayoutConfig::default();
    let areas = terminal_content_areas(&app, screen, &layout);
    assert_eq!(areas.len(), 2);

    // A cell inside each pane's content rect resolves to that pane.
    for (id, rect) in &areas {
        let hit = pane_at(&app, screen, &layout, rect.x, rect.y);
        assert_eq!(hit, Some((*id, *rect)));
    }
    // The project tab row owns row 0, and the upper panels the rows just
    // below it — neither is a pane.
    assert_eq!(pane_at(&app, screen, &layout, 0, 0), None);
    assert_eq!(pane_at(&app, screen, &layout, 0, 1), None);
    // ...and so do the two chrome rows at the bottom.
    assert_eq!(pane_at(&app, screen, &layout, 0, 39), None);
}

#[test]
fn upper_panel_at_resolves_list_and_diff_by_the_layout_split() {
    let app = app_with_files(vec!["a.rs"]);
    let screen = Rect::new(0, 0, 100, 40);
    let layout = LayoutConfig::default();

    // Row 0 is the project tab row, so the body starts at row 1. The
    // default file_list_pct (25) puts x=0 in the list and x=60 in the diff.
    assert_eq!(upper_panel_at(&app, screen, &layout, 0, 0), None);
    assert_eq!(
        upper_panel_at(&app, screen, &layout, 0, 1),
        Some(Focus::FileList)
    );
    assert_eq!(
        upper_panel_at(&app, screen, &layout, 60, 1),
        Some(Focus::DiffViewer)
    );
    // Below the upper panels: the terminal panel, then the two chrome
    // rows (notice, hint) — none of them is an upper panel.
    assert_eq!(upper_panel_at(&app, screen, &layout, 0, 37), None);
    assert_eq!(upper_panel_at(&app, screen, &layout, 0, 38), None);
    assert_eq!(upper_panel_at(&app, screen, &layout, 0, 39), None);
}

#[test]
fn upper_panel_at_misses_in_every_fullscreen_state() {
    // The implementation guards three distinct flags; each must miss on
    // its own, at a cell that hits the file list in the normal split.
    let screen = Rect::new(0, 0, 100, 40);
    let layout = LayoutConfig::default();

    let mut diff_full = app_with_files(vec!["a.rs"]);
    diff_full.toggle_diff_fullscreen();
    assert_eq!(upper_panel_at(&diff_full, screen, &layout, 0, 1), None);

    let mut list_full = app_with_files(vec!["a.rs"]);
    list_full.list_fullscreen = true;
    assert_eq!(upper_panel_at(&list_full, screen, &layout, 0, 1), None);

    let mut term_full = app_with_files(vec!["a.rs"]);
    term_full.terminal.fullscreen = TerminalFullscreen::Grid;
    assert_eq!(upper_panel_at(&term_full, screen, &layout, 0, 1), None);
}

#[test]
fn pane_at_misses_when_another_panel_is_fullscreen() {
    let mut app = app_with_files(vec!["a.rs"]);
    app.terminal.panes.push(crate::app::PaneInfo {
        id: 1,
        title: "shell".to_string(),
    });
    app.toggle_diff_fullscreen();

    let hit = pane_at(
        &app,
        Rect::new(0, 0, 100, 40),
        &LayoutConfig::default(),
        50,
        30,
    );

    assert_eq!(hit, None);
}
