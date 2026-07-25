use super::*;

#[test]
fn ensure_initial_terminal_opens_single_empty_pane_without_commands() {
    let mut app = app_with_fake_backend();
    app.ensure_initial_terminal(&[]);
    assert_eq!(app.terminal.panes.len(), 1);
    assert_eq!(app.terminal.panes[0].title, "shell 1");
}

#[test]
fn ensure_initial_terminal_focuses_first_pane_on_fresh_launch() {
    let mut app = app_with_fake_backend();
    // Helpers construct with FileList focus; a fresh launch must hand the
    // first pane the input focus so keystrokes reach the terminal program.
    assert_eq!(app.focus, Focus::FileList);
    app.ensure_initial_terminal(&[]);
    assert_eq!(app.focus, Focus::Terminal);
    assert_eq!(app.terminal.active, 0);
}

#[test]
fn restore_session_overrides_fresh_launch_terminal_focus() {
    let mut app = app_with_fake_backend();
    app.ensure_initial_terminal(&[]);
    assert_eq!(app.focus, Focus::Terminal);
    // A restored session must win over the fresh-launch terminal focus.
    app.restore_session(&crate::workspace::persistence::SessionState {
        focus: Some(Focus::FileList),
        ..Default::default()
    });
    assert_eq!(app.focus, Focus::FileList);
}

#[test]
fn restore_pane_focus_wins_immediately_without_snapshot() {
    // The synchronous focus restore (run at startup before the first
    // snapshot) must already override the fresh-launch terminal focus, so
    // no keystroke is ever routed to the terminal on a FileList restart.
    let mut app = app_with_fake_backend();
    app.ensure_initial_terminal(&[]);
    assert_eq!(app.focus, Focus::Terminal);
    app.restore_pane_focus(&crate::workspace::persistence::SessionState {
        focus: Some(Focus::FileList),
        ..Default::default()
    });
    assert_eq!(app.focus, Focus::FileList);
}

#[test]
fn ensure_initial_terminal_opens_one_pane_per_startup_command() {
    use crate::config::StartupCommand;
    let mut app = app_with_fake_backend();
    let commands = vec![
        StartupCommand {
            name: Some("Claude".into()),
            command: "claude".into(),
        },
        StartupCommand {
            name: None,
            command: "cargo test".into(),
        },
    ];
    app.ensure_initial_terminal(&commands);
    assert_eq!(app.terminal.panes.len(), 2);
    assert_eq!(app.terminal.panes[0].title, "Claude");
    assert_eq!(app.terminal.panes[1].title, "cargo test");
    // Focus clamps to the first reserved pane.
    assert_eq!(app.terminal.active, 0);
}
