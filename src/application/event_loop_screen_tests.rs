//! The event loop's cached screen size, as seen through input dispatch.

use crate::application::input::dispatch::KeyOutcome;
use crate::application::input::events::dispatch_input_event;
use crate::application::redraw::RedrawState;
use crate::application::screen_geometry::ScreenGeometry;
use crate::config::Config;
use crate::workspace::Workspace;
use crossterm::event::{Event, KeyCode, KeyEvent, KeyModifiers};
use ratatui::Terminal;
use ratatui::backend::TestBackend;
use ratatui::layout::Rect;
use std::time::Instant;

fn leader_key() -> KeyEvent {
    KeyEvent::new(KeyCode::Char('f'), KeyModifiers::CONTROL)
}

#[test]
fn a_resize_event_updates_the_cached_screen_size_without_a_measurement() {
    let mut app = crate::app::tests::app_with_fake_backend();
    app.terminal.create_pane_now().unwrap();
    app.poll_terminal();
    let mut workspace = Workspace::new(leader_key());
    workspace.add(app);
    let cfg = Config::default();
    let mut terminal = Terminal::new(TestBackend::new(100, 40)).unwrap();
    let mut redraw = RedrawState::new();
    let mut geometry = ScreenGeometry::default();
    let old_screen = geometry.screen(|| Ok(terminal.size()?)).unwrap();
    assert_eq!(old_screen, Rect::new(0, 0, 100, 40));

    terminal.backend_mut().resize(50, 20);
    let outcome = dispatch_input_event(
        &terminal,
        &mut workspace,
        &cfg,
        Event::Resize(50, 20),
        &mut redraw,
        &mut geometry,
        Instant::now(),
    )
    .unwrap();

    assert!(outcome.is_none());
    assert!(redraw.is_dirty(), "a resize still asks for a frame");
    // No measurement: the event carried the size, and the layout below is built
    // for it rather than for the pre-resize screen.
    let screen = geometry
        .screen(|| unreachable!("a resize event must not send the loop back to the backend"))
        .unwrap();
    assert_eq!(screen, Rect::new(0, 0, 50, 20));
    let app = workspace.active().unwrap();
    let resized = geometry.pane_sizes(app, &cfg.layout, screen).to_vec();
    let before = geometry.pane_sizes(app, &cfg.layout, old_screen).to_vec();
    assert_ne!(resized, before, "panes are laid out for the new screen");
}

#[test]
fn a_mouse_event_hit_tests_against_the_cached_size() {
    // The first mouse event measures once; a resize event then moves the
    // hit-testing frame without another backend call.
    let mut app = crate::app::tests::app_with_fake_backend();
    app.terminal.create_pane_now().unwrap();
    app.poll_terminal();
    app.focus = crate::app::Focus::Terminal;
    let target = app.terminal.panes[0].id;
    let mut workspace = Workspace::new(leader_key());
    workspace.add(app);
    let cfg = Config::default();
    let mut terminal = Terminal::new(TestBackend::new(100, 40)).unwrap();
    let mut redraw = RedrawState::new();
    let mut geometry = ScreenGeometry::default();

    let press = || {
        Event::Mouse(crossterm::event::MouseEvent {
            kind: crossterm::event::MouseEventKind::Down(crossterm::event::MouseButton::Left),
            column: 2,
            row: 30,
            modifiers: KeyModifiers::NONE,
        })
    };
    dispatch_input_event(
        &terminal,
        &mut workspace,
        &cfg,
        press(),
        &mut redraw,
        &mut geometry,
        Instant::now(),
    )
    .unwrap();

    // That column is inside the pane on a 100x40 screen; the same event on a
    // 20x8 screen falls outside it.
    let narrow = Rect::new(0, 0, 20, 8);
    let wide = Rect::new(0, 0, 100, 40);
    {
        let app = workspace.active().unwrap();
        assert!(
            crate::ui::pane_at(app, wide, &cfg.layout, 2, 30).is_some(),
            "the press starts inside the pane"
        );
        assert!(
            crate::ui::pane_at(app, narrow, &cfg.layout, 2, 30).is_none(),
            "the press must not be inside the pane on the narrow screen"
        );
    }
    terminal.backend_mut().resize(20, 8);
    let outcome = dispatch_input_event(
        &terminal,
        &mut workspace,
        &cfg,
        Event::Resize(20, 8),
        &mut redraw,
        &mut geometry,
        Instant::now(),
    )
    .unwrap();
    assert!(outcome.is_none());

    let outcome = dispatch_input_event(
        &terminal,
        &mut workspace,
        &cfg,
        press(),
        &mut redraw,
        &mut geometry,
        Instant::now(),
    )
    .unwrap();
    assert!(matches!(outcome, Some(KeyOutcome::Continue)));
    assert_eq!(
        geometry
            .screen(|| unreachable!("the cached size is current"))
            .unwrap(),
        narrow
    );
    assert_eq!(
        workspace.active().unwrap().terminal.active_pane_id(),
        Some(target)
    );
}
