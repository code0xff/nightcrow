//! Frame requests from terminal events that do not change the split window.

use crate::application::event_loop::poll_terminals_before_input;
use crate::application::redraw::RedrawState;
use crate::backend::{BackendEvent, PaneId};
use crate::runtime::terminal::{MAX_VISIBLE_NORMAL, TerminalFullscreen, TerminalState};
use crate::test_util::FakeBackend;
use crate::workspace::Workspace;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use std::cell::RefCell;
use std::rc::Rc;

type EventQueue = Rc<RefCell<Vec<BackendEvent>>>;

struct Fixture {
    workspace: Workspace,
    events: EventQueue,
    hidden: PaneId,
}

impl Fixture {
    /// One project whose terminal holds one more pane than the split window
    /// shows, settled so that only an injected event can dirty a frame.
    fn new() -> Self {
        let backend = FakeBackend::default();
        let events = backend.pending_events.clone();
        let mut app = crate::app::tests::app_with_files(Vec::new());
        app.terminal = TerminalState::new(Some(Box::new(backend)), false);
        for _ in 0..=MAX_VISIBLE_NORMAL {
            app.terminal.create_pane_now().unwrap();
        }
        let hidden = app.terminal.panes[MAX_VISIBLE_NORMAL].id;
        let mut workspace =
            Workspace::new(KeyEvent::new(KeyCode::Char('f'), KeyModifiers::CONTROL));
        workspace.add(app);
        let mut redraw = RedrawState::new();
        poll_terminals_before_input(&mut workspace, &mut redraw);
        assert!(
            !workspace.projects()[0].terminal.pane_is_visible(hidden),
            "the extra pane must start outside the split window"
        );
        Self {
            workspace,
            events,
            hidden,
        }
    }

    fn visible(&self) -> PaneId {
        self.workspace.projects()[0].terminal.panes[0].id
    }

    fn poll(&mut self, event: BackendEvent) -> bool {
        self.events.borrow_mut().push(event);
        let mut redraw = RedrawState::new();
        redraw.take();
        poll_terminals_before_input(&mut self.workspace, &mut redraw);
        redraw.take()
    }
}

#[test]
fn hidden_pane_output_does_not_request_a_frame() {
    let mut fixture = Fixture::new();
    let hidden = fixture.hidden;

    let changed = fixture.poll(BackendEvent::Output {
        pane: hidden,
        data: b"hidden".to_vec(),
    });

    assert!(!changed, "hidden-pane output must not repaint the body");
    let mut text = String::new();
    fixture.workspace.projects()[0]
        .terminal
        .screen_for_pane(hidden)
        .unwrap()
        .cell(0, 0)
        .unwrap()
        .append_contents(&mut text);
    assert_eq!(text, "h", "hidden output still reaches the emulator");
}

#[test]
fn hidden_pane_resize_does_not_request_a_frame() {
    let mut fixture = Fixture::new();
    let hidden = fixture.hidden;

    let changed = fixture.poll(BackendEvent::Resized {
        pane: hidden,
        rows: 10,
        cols: 20,
    });

    assert!(!changed, "a hidden pane's confirmed size is not on screen");
}

#[test]
fn zoom_hides_the_other_panes_and_their_output() {
    let mut fixture = Fixture::new();
    let other = fixture.workspace.projects()[0].terminal.panes[1].id;
    assert!(
        fixture.poll(BackendEvent::Output {
            pane: other,
            data: b"grid".to_vec(),
        }),
        "every pane in the split grid repaints"
    );

    fixture.workspace.projects_mut()[0].terminal.fullscreen = TerminalFullscreen::Zoom;
    fixture.workspace.projects_mut()[0]
        .terminal
        .sync_visible_window();

    assert!(
        !fixture.poll(BackendEvent::Output {
            pane: other,
            data: b"zoomed away".to_vec(),
        }),
        "zoom shows only the active pane"
    );
}

#[test]
fn visible_pane_output_requests_a_frame() {
    let mut fixture = Fixture::new();
    let visible = fixture.visible();

    let changed = fixture.poll(BackendEvent::Output {
        pane: visible,
        data: b"shown".to_vec(),
    });

    assert!(changed, "visible-pane output must repaint the body");
}

#[test]
fn pane_title_change_requests_a_frame() {
    let mut fixture = Fixture::new();
    let hidden = fixture.hidden;

    let changed = fixture.poll(BackendEvent::Output {
        pane: hidden,
        data: b"\x1b]2;renamed\x07".to_vec(),
    });

    assert!(changed, "a title change is visible in the pane tab row");
    assert_eq!(
        fixture.workspace.projects()[0].terminal.panes[MAX_VISIBLE_NORMAL].title,
        "renamed"
    );
}

#[test]
fn pane_attention_and_exit_request_a_frame() {
    let mut fixture = Fixture::new();
    let hidden = fixture.hidden;

    assert!(
        fixture.poll(BackendEvent::Attention { pane: hidden }),
        "a new unread badge must repaint the tabs"
    );
    assert!(
        fixture.poll(BackendEvent::Exited { pane: hidden }),
        "closing a pane changes the pane tab row"
    );
    assert_eq!(
        fixture.workspace.projects()[0].terminal.panes.len(),
        MAX_VISIBLE_NORMAL
    );
}
