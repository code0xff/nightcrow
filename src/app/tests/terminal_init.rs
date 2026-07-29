//! What happens to focus and the restored session when the panes turn up.
//!
//! Panes belong to the session the daemon owns, so a fresh project view has
//! none: they arrive over the connection, after the view has been built and a
//! saved session applied to it. Everything here is about that gap.

use super::*;

/// A pane arriving from the session, taken delivery of the way a frame does.
fn pane_arrives(app: &mut App) {
    app.terminal.create_pane().expect("asks for a pane");
    app.poll_terminal();
}

#[test]
fn the_first_pane_to_arrive_takes_the_input_focus_on_a_fresh_launch() {
    // Otherwise a fresh attach would sit on the file list with terminals on
    // screen, and the first keystroke would go somewhere the user is not
    // looking.
    let mut app = app_with_fake_backend();
    assert_eq!(app.focus, Focus::FileList, "nothing to focus yet");

    pane_arrives(&mut app);

    assert_eq!(app.focus, Focus::Terminal);
    assert_eq!(app.terminal.active, 0);
}

#[test]
fn a_restored_focus_survives_the_panes_arriving_later() {
    // The saved focus is applied before any pane exists, so the fresh-launch
    // rule must not overwrite it when they show up.
    let mut app = app_with_fake_backend();
    app.restore_session(&crate::workspace::persistence::SessionState {
        focus: Some(Focus::FileList),
        ..Default::default()
    });

    pane_arrives(&mut app);

    assert_eq!(app.focus, Focus::FileList);
}

#[test]
fn a_restored_terminal_focus_waits_for_the_pane_it_points_at() {
    // Focusing the terminal against an empty pane list would route keystrokes
    // at nothing, so the restore is held until there is a pane to focus.
    let mut app = app_with_fake_backend();
    app.restore_session(&crate::workspace::persistence::SessionState {
        focus: Some(Focus::Terminal),
        ..Default::default()
    });
    assert_eq!(app.focus, Focus::FileList, "not yet");

    pane_arrives(&mut app);

    assert_eq!(app.focus, Focus::Terminal);
}

#[test]
fn a_restored_active_pane_is_applied_once_that_many_panes_are_open() {
    let mut app = app_with_fake_backend();
    app.terminal.create_pane().expect("asks");
    app.terminal.create_pane().expect("asks");
    app.restore_session(&crate::workspace::persistence::SessionState {
        active_pane: 1,
        focus: Some(Focus::Terminal),
        ..Default::default()
    });
    assert_eq!(app.terminal.active, 0, "no panes to choose between yet");

    app.poll_terminal();

    assert_eq!(app.terminal.panes.len(), 2);
    assert_eq!(app.terminal.active, 1);
}

#[test]
fn a_restored_fullscreen_panel_waits_for_the_panes_too() {
    let mut app = app_with_fake_backend();
    app.restore_session(&crate::workspace::persistence::SessionState {
        terminal_fullscreen: true,
        ..Default::default()
    });
    assert_eq!(app.terminal.fullscreen, TerminalFullscreen::Off);

    pane_arrives(&mut app);

    assert_eq!(app.terminal.fullscreen, TerminalFullscreen::Grid);
}

#[test]
fn a_pane_arriving_after_the_restore_is_applied_leaves_the_focus_alone() {
    // The restore is spent on the first pane. A pane opened later — by this
    // client or another — must not re-apply it and drag the user back.
    let mut app = app_with_fake_backend();
    pane_arrives(&mut app);
    app.focus = Focus::DiffViewer;

    pane_arrives(&mut app);

    assert_eq!(app.focus, Focus::DiffViewer);
    assert_eq!(app.terminal.panes.len(), 2);
}
