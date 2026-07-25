use super::helpers::*;
use crate::app::Focus;
use crate::app::tests::app_with_files;
use crate::application::input::dispatch::{KeyOutcome, ProjectRequest, dispatch_key, handle_key};
use crate::workspace::Workspace;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

#[test]
fn handle_key_ignores_release_events() {
    // Regression for 4faacce: Windows / kitty keyboard protocol emits
    // Press+Release pairs for every keystroke. Only Press must trigger
    // app mutations; a Release must never act.
    let mut app = app_with_files(vec!["a.rs"]);
    let release = KeyEvent::new_with_kind(
        KeyCode::Char('f'),
        KeyModifiers::CONTROL,
        crossterm::event::KeyEventKind::Release,
    );

    let outcome = handle_key(&mut app, release);

    assert!(matches!(outcome, KeyOutcome::Continue));
}

#[test]
fn handle_key_leader_then_q_quits() {
    let mut app = app_with_files(vec!["a.rs"]);

    let first = handle_key(&mut app, leader());
    assert!(matches!(first, KeyOutcome::Continue));
    assert!(app.prefix_armed(), "leader must arm the prefix");

    let second = handle_key(&mut app, press(KeyCode::Char('q'), KeyModifiers::NONE));
    assert!(matches!(second, KeyOutcome::Quit));
    assert!(!app.prefix_armed(), "prefix must disarm after follow-up");
}

#[test]
fn handle_key_bare_ctrl_f_arms_prefix_and_does_not_quit() {
    // Ctrl+F is the default leader: pressing it alone arms the prefix and
    // never quits nightcrow on its own (quitting is `<leader> q`).
    let mut app = app_with_terminal_pane();

    let outcome = handle_key(&mut app, press(KeyCode::Char('f'), KeyModifiers::CONTROL));

    assert!(matches!(outcome, KeyOutcome::Continue));
    assert!(app.prefix_armed(), "the leader press arms the prefix");
}

#[test]
fn leader_x_asks_the_workspace_to_close_the_project() {
    let mut app = app_with_files(vec!["a.rs"]);
    let _ = handle_key(&mut app, leader());

    let outcome = handle_key(&mut app, press(KeyCode::Char('x'), KeyModifiers::NONE));

    assert_eq!(outcome, KeyOutcome::Project(ProjectRequest::Close));
    assert!(!app.prefix_armed(), "prefix must disarm after follow-up");
}

#[test]
fn leader_o_asks_the_workspace_to_raise_the_dialog() {
    let mut app = app_with_files(vec!["a.rs"]);
    let _ = handle_key(&mut app, leader());

    let outcome = handle_key(&mut app, press(KeyCode::Char('o'), KeyModifiers::NONE));

    // The dialog is workspace state, so a handler holding one project can
    // only ask for it.
    assert_eq!(outcome, KeyOutcome::Project(ProjectRequest::OpenDialog));
}

#[test]
fn a_doubled_leader_on_the_empty_screen_does_not_quit() {
    // `<L> <L>` sends a literal leader to a pane on the project screen.
    // Here there is none, but the follow-up must still not reach the action
    // table: with the default ctrl+f leader it would match `f` and toggle
    // fullscreen.
    let mut ws = Workspace::new(leader());

    let _ = dispatch_key(&mut ws, leader());
    let outcome = dispatch_key(&mut ws, leader());

    assert_eq!(outcome, KeyOutcome::Continue);
}

#[test]
fn handle_key_leader_esc_cancels() {
    let mut app = app_with_files(vec!["a.rs"]);
    let _ = handle_key(&mut app, leader());
    assert!(app.prefix_armed());

    let outcome = handle_key(&mut app, press(KeyCode::Esc, KeyModifiers::NONE));
    assert!(matches!(outcome, KeyOutcome::Continue));
    assert!(!app.prefix_armed(), "Esc must cancel the armed prefix");
}

#[test]
fn handle_key_leader_ctrl_c_cancels() {
    let mut app = app_with_terminal_pane();
    let _ = handle_key(&mut app, leader());
    assert!(app.prefix_armed());

    let outcome = handle_key(&mut app, press(KeyCode::Char('c'), KeyModifiers::CONTROL));
    assert!(matches!(outcome, KeyOutcome::Continue));
    assert!(!app.prefix_armed(), "Ctrl+C must cancel the armed prefix");
    // The cancel is consumed, never leaked to the PTY.
    assert!(
        backend_payloads(&app).is_empty(),
        "Ctrl+C cancel must not send bytes to the PTY"
    );
}

#[test]
fn handle_key_ctrl_super_leader_passes_through() {
    // A Super/Hyper/Meta bit on top of Ctrl+<leader> (enhanced keyboard
    // protocols report these) is a different chord, so it must reach the
    // PTY rather than arm the prefix.
    let mut app = app_with_terminal_pane();

    let outcome = handle_key(
        &mut app,
        press(
            KeyCode::Char('f'),
            KeyModifiers::CONTROL | KeyModifiers::SUPER,
        ),
    );

    assert!(matches!(outcome, KeyOutcome::Continue));
    assert!(
        !app.prefix_armed(),
        "Ctrl+Super+leader must not arm the prefix"
    );
}

#[test]
fn handle_key_ctrl_alt_leader_passes_through() {
    // Ctrl+Alt+<leader> carries an extra modifier, so it is NOT the leader
    // chord — it must reach the PTY rather than arm the prefix.
    let mut app = app_with_terminal_pane();

    let outcome = handle_key(
        &mut app,
        press(
            KeyCode::Char('f'),
            KeyModifiers::CONTROL | KeyModifiers::ALT,
        ),
    );

    assert!(matches!(outcome, KeyOutcome::Continue));
    assert!(
        !app.prefix_armed(),
        "Ctrl+Alt+leader must not arm the prefix"
    );
    assert!(
        !backend_payloads(&app).is_empty(),
        "Ctrl+Alt+leader must pass through to the PTY"
    );
}

#[test]
fn leader_leader_sends_literal_leader_even_when_leader_is_ctrl_c() {
    // With a `ctrl+c` leader, `<leader><leader>` must still reach the PTY
    // as a literal Ctrl+C (0x03); the leader-again path takes precedence
    // over the Ctrl+C cancel path.
    let mut app = app_with_terminal_pane();
    app.leader = press(KeyCode::Char('c'), KeyModifiers::CONTROL);

    let _ = handle_key(&mut app, press(KeyCode::Char('c'), KeyModifiers::CONTROL));
    assert!(app.prefix_armed());

    let outcome = handle_key(&mut app, press(KeyCode::Char('c'), KeyModifiers::CONTROL));
    assert!(matches!(outcome, KeyOutcome::Continue));
    assert!(!app.prefix_armed());
    assert_eq!(
        backend_payloads(&app).concat(),
        vec![0x03],
        "<leader><leader> must deliver a literal Ctrl+C to the PTY"
    );
}

#[test]
fn handle_key_leader_unmapped_followup_cancels() {
    let mut app = app_with_terminal_pane();
    let _ = handle_key(&mut app, leader());
    assert!(app.prefix_armed());

    let outcome = handle_key(&mut app, press(KeyCode::Char('z'), KeyModifiers::NONE));
    assert!(matches!(outcome, KeyOutcome::Continue));
    assert!(!app.prefix_armed());
    // The unmapped follow-up is consumed, NOT forwarded to the PTY.
    assert!(
        backend_payloads(&app).is_empty(),
        "unmapped follow-up must be dropped, not sent to the PTY"
    );
}

#[test]
fn handle_key_double_leader_sends_literal_to_pty() {
    let mut app = app_with_terminal_pane();
    let _ = handle_key(&mut app, leader());
    assert!(app.prefix_armed());

    let outcome = handle_key(&mut app, leader());
    assert!(matches!(outcome, KeyOutcome::Continue));
    assert!(!app.prefix_armed());
    // Ctrl+F encodes to 0x06 (ACK) — the literal leader byte.
    assert_eq!(backend_payloads(&app), vec![vec![0x06]]);
}

#[test]
fn handle_key_leader_t_opens_pane() {
    let mut app = app_with_terminal_pane();
    let before = app.terminal.panes.len();
    let _ = handle_key(&mut app, leader());
    let _ = handle_key(&mut app, press(KeyCode::Char('t'), KeyModifiers::NONE));
    assert_eq!(app.terminal.panes.len(), before + 1);
}

#[test]
fn handle_key_leader_w_closes_pane_with_terminal_focus() {
    let mut app = app_with_terminal_pane();
    app.terminal.create_pane().unwrap();
    let before = app.terminal.panes.len();
    let _ = handle_key(&mut app, leader());
    let _ = handle_key(&mut app, press(KeyCode::Char('w'), KeyModifiers::NONE));
    assert_eq!(app.terminal.panes.len(), before - 1);
}

#[test]
fn handle_key_leader_w_closes_pane_in_terminal_fullscreen() {
    // Fullscreen routes the follow-up through `prefix_action_fullscreen`;
    // `w` must keep closing there (focus is Terminal while it fills the
    // body).
    let mut app = app_with_terminal_pane();
    app.terminal.create_pane().unwrap();
    app.terminal.fullscreen = crate::runtime::terminal::TerminalFullscreen::Grid;
    let before = app.terminal.panes.len();

    let _ = handle_key(&mut app, leader());
    let _ = handle_key(&mut app, press(KeyCode::Char('w'), KeyModifiers::NONE));

    assert_eq!(app.terminal.panes.len(), before - 1);
}

#[test]
fn handle_key_leader_w_is_ignored_without_terminal_focus() {
    // Without terminal focus the active pane is rendered identically to
    // the others, so `<leader> w` must not close an invisible target.
    // The follow-up is still consumed: prefix disarmed, nothing forwarded.
    let mut app = app_with_terminal_pane();
    app.focus = Focus::FileList;
    let before = app.terminal.panes.len();

    let _ = handle_key(&mut app, leader());
    let _ = handle_key(&mut app, press(KeyCode::Char('w'), KeyModifiers::NONE));

    assert_eq!(
        app.terminal.panes.len(),
        before,
        "leader+w must be a no-op outside terminal focus"
    );
    assert!(!app.prefix_armed());
    assert!(
        backend_payloads(&app).is_empty(),
        "the consumed follow-up must not reach the PTY"
    );
}

#[test]
fn handle_key_leader_l_toggles_log_view_from_upper_focus() {
    // Leader commands work in upper (file list) focus too, not just
    // terminal focus.
    let mut app = app_with_files(vec!["a.rs"]);
    app.focus = Focus::FileList;
    let before = app.mode;
    let _ = handle_key(&mut app, leader());
    let _ = handle_key(&mut app, press(KeyCode::Char('l'), KeyModifiers::NONE));
    assert_ne!(
        app.mode, before,
        "leader+l must toggle the view in upper focus"
    );
}
