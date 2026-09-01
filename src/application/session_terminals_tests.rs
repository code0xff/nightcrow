//! The client end of a shared terminal session, against a real daemon.
//!
//! One test with a real session behind it, because everything this step added
//! only exists between the parts: the tab is built with the repository's end of
//! the connection, a pane request crosses to the daemon, and the pane that
//! comes back has to reach that tab's emulator with its output. A fake at any
//! of those seams would assert the seam rather than the crossing.

use crate::application::input::dispatch::ProjectContext;
use crate::application::session_link::SessionLink;
use crate::daemon::client::DaemonClient;
use crate::daemon::socket::DaemonSocket;
use crate::workspace::Workspace;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use std::time::{Duration, Instant};

/// Generous, and only the failure verdict waits: the session spawns the user's
/// real `$SHELL`, which an interactive zsh answers by sourcing its whole rc
/// chain. Matches the hub's own shell tests.
const DEADLINE: Duration = Duration::from_secs(15);

/// A daemon serving `repos`, and a client attached to it.
///
/// The socket comes back with them: it owns the unlink and the instance lock, so
/// dropping it would free the path the client is attached to.
fn attached(dir: &tempfile::TempDir, repos: &[String]) -> (DaemonSocket, DaemonClient) {
    let socket = DaemonSocket::bind(&dir.path().join("d.sock")).expect("binds");
    let listener = socket.listener().try_clone().expect("clones");
    let state = crate::test_util::session_state(repos, dir.path());
    let (shutdown_tx, _shutdown_rx) = std::sync::mpsc::sync_channel(1);
    let session = crate::daemon::serve::start(
        state,
        socket.path(),
        "127.0.0.1:4321".parse().unwrap(),
        shutdown_tx,
    )
    .expect("starts the watcher");
    std::thread::spawn(move || crate::daemon::serve::serve(listener, session));
    let client = DaemonClient::connect(socket.path()).expect("attaches");
    (socket, client)
}

/// Run the tick the main loop runs — take in what the daemon said, then let
/// every tab drain its panes — until `done`, and report whether it happened.
fn tick_until(
    link: &mut SessionLink,
    ws: &mut Workspace,
    ctx: &ProjectContext,
    mut done: impl FnMut(&mut Workspace) -> bool,
) -> bool {
    let deadline = Instant::now() + DEADLINE;
    while Instant::now() < deadline {
        link.sync(ws, ctx);
        for project in ws.projects_mut() {
            project.poll_terminal();
        }
        if done(ws) {
            return true;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    false
}

/// Everything the active pane's emulator is currently showing.
fn screen_text(app: &crate::app::App) -> String {
    let Some(view) = app.active_screen() else {
        return String::new();
    };
    let (rows, cols) = view.size();
    let mut out = String::new();
    for row in 0..rows {
        for col in 0..cols {
            if let Some(cell) = view.cell(row, col) {
                cell.append_contents(&mut out);
            }
        }
    }
    out
}

#[test]
fn a_tab_shows_the_pane_the_session_is_running_and_the_output_it_produces() {
    let (repo, path) = crate::test_util::make_repo();
    let dir = tempfile::TempDir::new().unwrap();
    let (_socket, client) = attached(&dir, std::slice::from_ref(&path));
    let cfg = crate::config::Config::default();
    let leader = KeyEvent::new(KeyCode::Char('f'), KeyModifiers::CONTROL);
    let ctx = ProjectContext { cfg: &cfg, leader };
    let mut link = SessionLink::new(client);
    let mut ws = Workspace::new(leader);

    // The tab arrives from the session, not from this client's own state.
    assert!(
        tick_until(&mut link, &mut ws, &ctx, |ws| !ws.projects().is_empty()),
        "the daemon's repository never reached the client"
    );
    assert_eq!(ws.projects().len(), 1);

    // A project does not spend a process on a shell until somebody asks for it.
    let quiet_until = Instant::now() + Duration::from_millis(100);
    while Instant::now() < quiet_until {
        link.sync(&mut ws, &ctx);
        for project in ws.projects_mut() {
            project.poll_terminal();
        }
        std::thread::sleep(Duration::from_millis(5));
    }
    assert!(
        ws.active().is_some_and(|app| app.terminal.panes.is_empty()),
        "a default project must not auto-open a terminal"
    );

    ws.active_mut().expect("a tab").open_new_pane();
    assert!(
        tick_until(&mut link, &mut ws, &ctx, |ws| {
            ws.active()
                .is_some_and(|app| !app.terminal.panes.is_empty())
        }),
        "no pane arrived from the session"
    );

    // The shell says something as soon as it starts, and it has to land in this
    // tab's emulator — the client renders the grid itself, so bytes that stopped
    // at the socket would leave a pane that exists and shows nothing.
    assert!(
        tick_until(&mut link, &mut ws, &ctx, |ws| {
            ws.active()
                .is_some_and(|app| !screen_text(app).trim().is_empty())
        }),
        "the pane produced no output the client could render"
    );

    // The explicit-open rule, end to end: keystrokes go to the terminal that
    // just appeared rather than to the file list the view was built on.
    assert_eq!(
        ws.active().expect("a tab").focus,
        crate::app::Focus::Terminal
    );
    drop(repo);
}
