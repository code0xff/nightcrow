use crate::app::Focus;
use crate::application::event_loop::poll_terminals_before_input;
use crate::application::input::dispatch::KeyOutcome;
use crate::application::input::events::dispatch_input_event;
use crate::application::input::ready::{InputControl, process_ready_input};
use crate::application::redraw::RedrawState;
use crate::config::Config;
use crate::runtime::terminal::TerminalState;
use crate::test_util::FakeBackend;
use crate::workspace::Workspace;
use crossterm::event::{
    Event, KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind,
};
use ratatui::Terminal;
use ratatui::backend::TestBackend;
use ratatui::layout::Rect;
use std::time::Instant;

fn leader() -> Event {
    Event::Key(KeyEvent::new(KeyCode::Char('f'), KeyModifiers::CONTROL))
}

fn leader_key() -> KeyEvent {
    KeyEvent::new(KeyCode::Char('f'), KeyModifiers::CONTROL)
}

fn key(c: char) -> Event {
    Event::Key(KeyEvent::new(KeyCode::Char(c), KeyModifiers::NONE))
}

fn tick_input(
    terminal: &Terminal<TestBackend>,
    workspace: &mut Workspace,
    cfg: &Config,
    event: Event,
    redraw: &mut RedrawState,
) -> anyhow::Result<()> {
    poll_terminals_before_input(workspace, redraw);
    let mut ready = Some(event);
    let progress = process_ready_input(
        || Ok(ready.take()),
        |event| Ok(vec![event]),
        |event| {
            let outcome =
                dispatch_input_event(terminal, workspace, cfg, event, redraw, Instant::now())?;
            assert!(matches!(outcome, Some(KeyOutcome::Continue)));
            Ok(InputControl::Continue)
        },
    )?;
    assert_eq!(progress.processed, 1);
    Ok(())
}

#[test]
fn confirmed_pane_creation_is_polled_before_the_next_unix_input_event() {
    let mut app = crate::app::tests::app_with_fake_backend();
    app.terminal.create_pane_now().unwrap();
    app.poll_terminal();
    let mut workspace = Workspace::new(leader_key());
    workspace.add(app);
    let cfg = Config::default();
    let terminal = Terminal::new(TestBackend::new(80, 24)).unwrap();
    let mut redraw = RedrawState::new();
    let first_pane = workspace.active().unwrap().terminal.active_pane_id();

    tick_input(&terminal, &mut workspace, &cfg, leader(), &mut redraw).unwrap();
    tick_input(&terminal, &mut workspace, &cfg, key('t'), &mut redraw).unwrap();
    assert_eq!(workspace.active().unwrap().terminal.panes.len(), 1);

    tick_input(&terminal, &mut workspace, &cfg, key('x'), &mut redraw).unwrap();

    let app = workspace.active().unwrap();
    assert_eq!(app.terminal.panes.len(), 2);
    assert_ne!(app.terminal.active_pane_id(), first_pane);
    assert_eq!(app.terminal.fake_backend_sent().unwrap(), vec![vec![b'x']]);
}

#[test]
fn hidden_terminal_output_is_consumed_without_body_repaint_until_badge_changes() {
    let mut active = crate::app::tests::app_with_files(vec![]);
    active.focus = Focus::Terminal;
    let mut hidden = crate::app::tests::app_with_files(vec![]);
    let backend = FakeBackend::default();
    let events = backend.pending_events.clone();
    hidden.terminal = TerminalState::new(Some(Box::new(backend)), false);
    hidden.terminal.create_pane().unwrap();
    events.borrow_mut().extend([
        crate::backend::BackendEvent::Output {
            pane: 1,
            data: b"hidden output".to_vec(),
        },
        crate::backend::BackendEvent::Attention { pane: 1 },
    ]);
    let mut workspace = Workspace::new(leader_key());
    workspace.add(active);
    workspace.add(hidden);
    workspace.switch(0);
    let mut redraw = RedrawState::new();
    redraw.take();

    poll_terminals_before_input(&mut workspace, &mut redraw);

    assert!(
        !redraw.take(),
        "hidden output must not repaint the active body"
    );
    let hidden = &workspace.projects()[1];
    let mut first_cell = String::new();
    hidden
        .terminal
        .screen_for_pane(1)
        .unwrap()
        .cell(0, 0)
        .unwrap()
        .append_contents(&mut first_cell);
    assert_eq!(first_cell, "h", "hidden output still updates the emulator");
    assert!(hidden.terminal.has_unread_attention());
    let attention = [false, true];
    redraw.observe_attention(&attention, true);
    assert!(redraw.take(), "the new unread badge must repaint the tabs");

    events
        .borrow_mut()
        .push(crate::backend::BackendEvent::Output {
            pane: 1,
            data: b" again".to_vec(),
        });
    poll_terminals_before_input(&mut workspace, &mut redraw);
    redraw.observe_attention(&attention, true);
    assert!(
        !redraw.take(),
        "more hidden bytes do not repaint a stable badge"
    );
}

#[test]
fn mouse_hit_testing_uses_the_post_resize_terminal_size() {
    let mut app = crate::app::tests::app_with_fake_backend();
    app.terminal.create_pane_now().unwrap();
    app.terminal.create_pane_now().unwrap();
    app.poll_terminal();
    app.focus = Focus::Terminal;
    app.terminal.active = 1;
    let target = app.terminal.panes[0].id;
    let cfg = Config::default();
    let old_screen = Rect::new(0, 0, 100, 40);
    let new_screen = Rect::new(0, 0, 50, 20);
    let point = (0..new_screen.height)
        .flat_map(|y| (0..new_screen.width).map(move |x| (x, y)))
        .find(|&(x, y)| {
            crate::ui::pane_at(&app, new_screen, &cfg.layout, x, y)
                .is_some_and(|(pane, _)| pane == target)
                && !crate::ui::pane_at(&app, old_screen, &cfg.layout, x, y)
                    .is_some_and(|(pane, _)| pane == target)
        })
        .expect("resize changes the target pane hit area");
    let mut workspace = Workspace::new(leader_key());
    workspace.add(app);
    let mut terminal = Terminal::new(TestBackend::new(100, 40)).unwrap();
    terminal.backend_mut().resize(50, 20);
    let mut redraw = RedrawState::new();
    let now = Instant::now();

    dispatch_input_event(
        &terminal,
        &mut workspace,
        &cfg,
        Event::Resize(50, 20),
        &mut redraw,
        now,
    )
    .unwrap();
    dispatch_input_event(
        &terminal,
        &mut workspace,
        &cfg,
        Event::Mouse(MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Left),
            column: point.0,
            row: point.1,
            modifiers: KeyModifiers::NONE,
        }),
        &mut redraw,
        now,
    )
    .unwrap();

    assert_eq!(
        workspace.active().unwrap().terminal.active_pane_id(),
        Some(target)
    );
}
