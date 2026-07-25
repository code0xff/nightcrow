use super::*;

#[test]
fn clamp_active_pane_preserves_non_terminal_focus_on_last_exit() {
    // Regression for 56ced5f: when the last terminal pane self-exits
    // (Ctrl+D in the only shell), focus that wasn't on Terminal must
    // stay put. Previously the clamp unconditionally redirected to
    // DiffViewer, yanking focus away from a user reading the diff.
    let mut app = app_with_files(vec!["a.rs"]);
    app.focus = Focus::FileList;
    // No panes registered — simulate "last pane exited" path.
    app.terminal.panes.clear();

    app.clamp_active_pane_after_removal();

    assert_eq!(app.focus, Focus::FileList);
    assert_eq!(app.terminal.active, 0);
    assert!(!app.terminal.fullscreen.fills_body());
}

#[test]
fn clamp_active_pane_redirects_when_focus_was_terminal() {
    // Symmetric case: if focus *was* Terminal and the last pane
    // exits, we need to redirect to a non-terminal pane (DiffViewer)
    // so the user can still drive the UI.
    let mut app = app_with_files(vec!["a.rs"]);
    app.focus = Focus::Terminal;
    app.terminal.panes.clear();

    app.clamp_active_pane_after_removal();

    assert_eq!(app.focus, Focus::DiffViewer);
}
