use crate::app::tests::app_with_fake_backend;
use crate::application::input::dispatch::{KeyOutcome, dispatch_key};
use crate::application::size_activity::{claim_for_user_activity, is_user_activity};
use crate::workspace::Workspace;
use crossterm::event::{
    Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers, MouseButton, MouseEvent, MouseEventKind,
};

fn key(kind: KeyEventKind, code: KeyCode) -> Event {
    let mut key = KeyEvent::new(code, KeyModifiers::NONE);
    key.kind = kind;
    Event::Key(key)
}

fn mouse(kind: MouseEventKind) -> Event {
    Event::Mouse(MouseEvent {
        kind,
        column: 0,
        row: 0,
        modifiers: KeyModifiers::NONE,
    })
}

#[test]
fn direct_input_claims_sizing_but_passive_events_do_not() {
    for event in [
        key(KeyEventKind::Press, KeyCode::Char('x')),
        key(KeyEventKind::Repeat, KeyCode::Char('x')),
        Event::Paste("text".into()),
        mouse(MouseEventKind::Down(MouseButton::Left)),
        mouse(MouseEventKind::ScrollDown),
    ] {
        assert!(
            is_user_activity(&event),
            "expected user activity: {event:?}"
        );
    }
    for event in [
        key(KeyEventKind::Release, KeyCode::Char('x')),
        mouse(MouseEventKind::Up(MouseButton::Left)),
        mouse(MouseEventKind::Drag(MouseButton::Left)),
        mouse(MouseEventKind::Moved),
        Event::FocusGained,
        Event::FocusLost,
        Event::Resize(80, 24),
    ] {
        assert!(
            !is_user_activity(&event),
            "expected passive event: {event:?}"
        );
    }

    for already_owner in [false, true] {
        let mut app = app_with_fake_backend();
        app.terminal.owns_size = already_owner;
        let mut workspace =
            Workspace::new(KeyEvent::new(KeyCode::Char('f'), KeyModifiers::CONTROL));
        assert!(workspace.add(app));
        claim_for_user_activity(
            &mut workspace,
            &key(KeyEventKind::Press, KeyCode::Char('x')),
        );
        assert_eq!(
            workspace
                .active()
                .unwrap()
                .terminal
                .fake_backend_claim_requests(),
            Some(1),
            "actual input must ask even before a stale ownership loss is polled"
        );
        claim_for_user_activity(&mut workspace, &Event::Resize(81, 24));
        assert_eq!(
            workspace
                .active()
                .unwrap()
                .terminal
                .fake_backend_claim_requests(),
            Some(1),
            "a passive screen resize does not request ownership"
        );
    }
}

#[test]
fn leader_r_forces_a_claim_and_refit_for_owner_and_spectator() {
    for already_owner in [true, false] {
        let mut app = app_with_fake_backend();
        app.terminal.create_pane_now().unwrap();
        let pane = app.terminal.panes[0].id;
        app.terminal.resize_visible_panes(&[(pane, 24, 80)]);
        app.terminal.owns_size = already_owner;
        let mut workspace =
            Workspace::new(KeyEvent::new(KeyCode::Char('f'), KeyModifiers::CONTROL));
        assert!(workspace.add(app));

        assert!(matches!(
            dispatch_key(
                &mut workspace,
                KeyEvent::new(KeyCode::Char('f'), KeyModifiers::CONTROL)
            ),
            KeyOutcome::Continue
        ));
        assert!(matches!(
            dispatch_key(
                &mut workspace,
                KeyEvent::new(KeyCode::Char('r'), KeyModifiers::NONE)
            ),
            KeyOutcome::Redraw
        ));

        let terminal = &workspace.active().unwrap().terminal;
        assert!(terminal.confirmed_content_size.is_empty());
        assert_eq!(terminal.fake_backend_claim_requests(), Some(1));
    }
}
