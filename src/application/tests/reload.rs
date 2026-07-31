//! `<prefix> u` against a session that owns the config.

use super::helpers::*;
use crate::application::input::dispatch::{KeyOutcome, ProjectRequest, dispatch_key};
use crate::workspace::Workspace;
use crossterm::event::{KeyCode, KeyModifiers};

fn reload(ws: &mut Workspace) -> KeyOutcome {
    assert!(matches!(dispatch_key(ws, leader()), KeyOutcome::Continue));
    dispatch_key(ws, press(KeyCode::Char('u'), KeyModifiers::NONE))
}

#[test]
fn reloading_the_config_asks_the_session() {
    // The plugins and the startup list belong to the daemon, so there is nothing
    // this client could apply on its own.
    let mut ws = workspace_on(&["/a"]);

    let outcome = reload(&mut ws);

    assert!(matches!(
        outcome,
        KeyOutcome::Project(ProjectRequest::ReloadConfig)
    ));
}

#[test]
fn the_config_can_be_reloaded_with_no_project_open() {
    // This is the moment it is most worth doing: the startup list a reload
    // replaces takes effect on the next project opened, and none is open yet.
    let mut ws = Workspace::new(leader());

    let outcome = reload(&mut ws);

    assert!(matches!(
        outcome,
        KeyOutcome::Project(ProjectRequest::ReloadConfig)
    ));
}
