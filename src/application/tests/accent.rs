//! `<prefix> p` against a session that owns the colour.

use super::helpers::*;
use crate::application::input::dispatch::{KeyOutcome, ProjectRequest, dispatch_key};
use crate::workspace::Workspace;
use crossterm::event::{KeyCode, KeyModifiers};

fn cycle_accent(ws: &mut Workspace) -> KeyOutcome {
    assert!(matches!(dispatch_key(ws, leader()), KeyOutcome::Continue));
    dispatch_key(ws, press(KeyCode::Char('p'), KeyModifiers::NONE))
}

#[test]
fn cycling_the_accent_asks_the_session_instead_of_painting_locally() {
    // Painting first would make this client the only one showing the new colour
    // until the broadcast caught up — the flicker the tab switch avoids the same
    // way.
    let mut ws = workspace_on(&["/a"]);
    let before = ws.current_accent();

    let outcome = cycle_accent(&mut ws);

    assert!(matches!(
        outcome,
        KeyOutcome::Project(ProjectRequest::CycleAccent)
    ));
    assert_eq!(
        ws.current_accent(),
        before,
        "nothing moves until the answer"
    );
}

#[test]
fn the_accent_can_be_cycled_with_no_project_open() {
    // The empty screen is painted in the session's accent too, and other
    // clients that do have a tab up follow this keystroke.
    let mut ws = Workspace::new(leader());

    let outcome = cycle_accent(&mut ws);

    assert!(matches!(
        outcome,
        KeyOutcome::Project(ProjectRequest::CycleAccent)
    ));
}
