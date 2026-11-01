use crate::app::tests::{app_with_fake_backend, app_with_files};
use crate::application::screen_geometry::ScreenGeometry;
use crate::config::LayoutConfig;
use crate::runtime::terminal::TerminalFullscreen;
use ratatui::layout::{Rect, Size};
use std::cell::Cell;

const SCREEN: Rect = Rect::new(0, 0, 100, 40);

fn terminal_app(panes: usize) -> crate::app::App {
    let mut app = app_with_fake_backend();
    for _ in 0..panes {
        app.terminal.create_pane_now().unwrap();
    }
    app.poll_terminal();
    app
}

/// A `measure` closure that reports `size` and counts how often it ran, which
/// is exactly the backend call the cache exists to avoid.
fn measuring<'a>(calls: &'a Cell<usize>, size: Size) -> impl FnOnce() -> anyhow::Result<Size> + 'a {
    move || {
        calls.set(calls.get() + 1);
        Ok(size)
    }
}

#[test]
fn the_screen_is_measured_once_and_then_reused() {
    let mut geometry = ScreenGeometry::default();
    let calls = Cell::new(0);

    for _ in 0..5 {
        assert_eq!(
            geometry
                .screen(measuring(&calls, Size::new(120, 50)))
                .unwrap(),
            Rect::new(0, 0, 120, 50)
        );
    }
    assert_eq!(calls.get(), 1, "idle ticks must not ask the backend again");
}

#[test]
fn a_resize_event_replaces_the_cached_screen_without_measuring() {
    let mut geometry = ScreenGeometry::default();
    let calls = Cell::new(0);
    geometry
        .screen(measuring(&calls, Size::new(120, 50)))
        .unwrap();

    geometry.resized(80, 24);

    assert_eq!(
        geometry
            .screen(measuring(&calls, Size::new(120, 50)))
            .unwrap(),
        Rect::new(0, 0, 80, 24)
    );
    assert_eq!(calls.get(), 1, "the event already carried the new size");
}

#[test]
fn forgetting_the_size_measures_the_backend_again() {
    let mut geometry = ScreenGeometry::default();
    let calls = Cell::new(0);
    geometry
        .screen(measuring(&calls, Size::new(100, 40)))
        .unwrap();

    // `<leader> r` clears the backend, so a resize that never arrived must be
    // picked up here rather than leaving panes at the old geometry.
    geometry.forget_size();
    geometry
        .screen(measuring(&calls, Size::new(90, 30)))
        .unwrap();

    assert_eq!(calls.get(), 2);
    assert_eq!(
        geometry
            .screen(measuring(&calls, Size::new(10, 10)))
            .unwrap(),
        Rect::new(0, 0, 90, 30)
    );
}

#[test]
fn a_zero_dimension_never_collapses_the_cached_screen() {
    let mut geometry = ScreenGeometry::default();
    let calls = Cell::new(0);
    geometry
        .screen(measuring(&calls, Size::new(100, 40)))
        .unwrap();

    geometry.resized(0, 0);
    geometry.resized(120, 0);

    assert_eq!(
        geometry.screen(measuring(&calls, Size::new(5, 5))).unwrap(),
        SCREEN,
        "a transient zero keeps the last usable screen"
    );
}

#[test]
fn pane_sizes_are_rebuilt_only_when_something_they_depend_on_changes() {
    let app = terminal_app(2);
    let layout = LayoutConfig::default();
    let mut geometry = ScreenGeometry::default();

    let first = geometry.pane_sizes(&app, &layout, SCREEN).to_vec();
    for _ in 0..5 {
        assert_eq!(geometry.pane_sizes(&app, &layout, SCREEN), first);
    }
    assert_eq!(geometry.rebuilds(), 1, "an idle tick reuses the layout");

    // A screen resize moves the panes even though nothing else did.
    geometry.pane_sizes(&app, &layout, Rect::new(0, 0, 60, 20));
    assert_eq!(geometry.rebuilds(), 2);
}

#[test]
fn a_layout_change_rebuilds_the_pane_sizes() {
    let app = terminal_app(2);
    let mut geometry = ScreenGeometry::default();
    let wide_upper = LayoutConfig {
        upper_pct: 55,
        ..LayoutConfig::default()
    };
    let short_upper = LayoutConfig {
        upper_pct: 20,
        ..LayoutConfig::default()
    };

    let before = geometry.pane_sizes(&app, &wide_upper, SCREEN).to_vec();
    let after = geometry.pane_sizes(&app, &short_upper, SCREEN).to_vec();

    assert_ne!(before, after, "a shorter upper panel leaves taller panes");
    assert_eq!(geometry.rebuilds(), 2);
}

#[test]
fn a_new_pane_with_the_same_count_rebuilds_the_pane_sizes() {
    let mut app = terminal_app(2);
    let layout = LayoutConfig::default();
    let mut geometry = ScreenGeometry::default();
    let before = geometry.pane_sizes(&app, &layout, SCREEN).to_vec();

    // Closing a pane and opening another between two ticks keeps the count but
    // moves the rectangles to a new id; reusing the cached ids would resize the
    // wrong pane.
    let replacement = app.terminal.panes[1].id + 1_000;
    app.terminal.panes[1].id = replacement;
    let after = geometry.pane_sizes(&app, &layout, SCREEN).to_vec();

    assert_eq!(before.len(), after.len());
    assert_eq!(after[1].0, replacement);
    assert_eq!(geometry.rebuilds(), 2);
}

#[test]
fn the_fullscreen_and_mode_flags_reach_the_cached_layout() {
    let mut app = terminal_app(1);
    let layout = LayoutConfig::default();
    let mut geometry = ScreenGeometry::default();
    let split = geometry.pane_sizes(&app, &layout, SCREEN).to_vec();

    app.terminal.fullscreen = TerminalFullscreen::Grid;
    let fullscreen = geometry.pane_sizes(&app, &layout, SCREEN).to_vec();
    assert_ne!(
        split, fullscreen,
        "fullscreen gives the pane the whole body"
    );

    let mut list_app = app_with_files(vec!["a.rs"]);
    list_app.list_fullscreen = true;
    let hidden = geometry.pane_sizes(&list_app, &layout, SCREEN).to_vec();
    assert!(hidden.is_empty(), "a fullscreen list hides the terminal");
}
