//! `nightcrow attach`: run the TUI against a session the daemon owns.
//!
//! The difference from running standalone is where the tabs come from. Here
//! the daemon has them, so nothing is restored from the workspace file and
//! nothing is written back to it — the daemon is doing that. This client starts
//! with no projects and adopts the set the daemon volunteers on attach, which
//! is also how it learns about every change afterwards.

use crate::application::event_loop::{ProjectContext, main_loop};
use crate::application::session_link::SessionLink;
use crate::application::splash::{SplashOutcome, splash_loop};
use crate::application::terminal_guard::TerminalGuard;
use crate::daemon::client::DaemonClient;
use crate::workspace::Workspace;
use anyhow::Result;
use ratatui::{Terminal, backend::CrosstermBackend};
use std::io;
use syntect::highlighting::ThemeSet;

/// Attach to the daemon and run the TUI until the user leaves or it goes away.
pub(crate) fn run_attach() -> Result<()> {
    let client = DaemonClient::connect(&crate::daemon::socket::default_socket_path()?)?;

    let cfg = crate::config::load_config()?;
    // Parsed before the alternate screen so its error is readable. The
    // configured startup terminals are not read here at all: the daemon runs
    // them once for the whole session, and a client that resolved its own copy
    // would only be able to disagree with it.
    let leader = crate::config::parse_leader(&cfg.input.leader)?;

    // Anchored where the daemon's is, not at the working directory. The
    // relative default (`.nightcrow/logs`) resolved against the cwd would
    // create that directory inside whatever repository the client was started
    // from, and nightcrow writes nothing inside a repository it is only
    // reading.
    let _log_guard = crate::platform::logging::init_logging(
        &cfg.log,
        &crate::platform::paths::state_dir_anchor(),
    );
    tracing::info!("attached to the nightcrow daemon");

    let _guard = TerminalGuard::enter(cfg.mouse.enabled)?;
    let original_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let _ = crossterm::terminal::disable_raw_mode();
        let _ = crossterm::execute!(
            io::stdout(),
            crossterm::event::DisableMouseCapture,
            crossterm::terminal::LeaveAlternateScreen
        );
        original_hook(info);
    }));
    let mut terminal = Terminal::new(CrosstermBackend::new(io::stdout()))?;

    let ss = two_face::syntax::extra_newlines();
    let ts = ThemeSet::load_defaults();
    let ctx = ProjectContext { cfg: &cfg, leader };
    // Starts empty on purpose: the daemon's first message is the set, and
    // seeding from the workspace file would put tabs on screen that the session
    // does not have, only to close them a frame later.
    let mut ws = Workspace::new(leader);
    // View state is still this client's to remember — which file was selected
    // in a repository is not part of the shared session (see the plan's
    // shared/per-client boundary), so it is read from the same file as before.
    if let Some(stored) = crate::workspace::persistence::load_workspace() {
        ws.set_remembered(stored.sessions);
    }

    // The daemon's answer, taken from the set it volunteered while attaching —
    // read in passing rather than drained, since the message itself belongs to
    // `main_loop`, which builds the tabs from it. The file is the fallback for
    // a daemon that answered the handshake before volunteering anything, and
    // `[theme]` behind that names what a session with no stored colour starts
    // in.
    let session_accent = client.session_accent().unwrap_or_else(|| {
        crate::web::viewer::prefs::PrefsStore::load_seeded(cfg.theme.preset_index())
            .get()
            .accent
    });
    // The splash is not the only screen that draws before the daemon's first
    // set arrives. Without this the first frames of the main view would come up
    // in the default rather than the session's colour, and the splash that just
    // painted correctly would appear to change its mind.
    ws.set_accent_index(session_accent);

    if matches!(
        splash_loop(&mut terminal, session_accent)?,
        SplashOutcome::Quit
    ) {
        tracing::info!("nightcrow detached during splash");
        return Ok(());
    }
    // The view state is written whichever way the loop ends. Losing which file
    // was selected because the daemon stopped would be a second insult, and this
    // half of the session file is the client's own — see `persist_view_state`.
    let ended = main_loop(
        &mut terminal,
        &mut ws,
        &ss,
        &ts,
        &cfg,
        &ctx,
        SessionLink::new(client),
    );
    persist_view_state(&ws);
    ended?;
    tracing::info!("nightcrow detached");
    Ok(())
}

/// Write this client's view state back, leaving the tab list alone.
///
/// The file has two halves and two owners: the daemon writes which
/// repositories are open and which is active, and a client writes what it had
/// selected and where it had scrolled. Read-modify-write rather than a whole
/// rewrite, so detaching cannot roll the session's tab list back to whatever
/// this client happened to be showing.
///
/// The two can still race — a client detaching in the same instant a
/// repository is opened elsewhere can lose that open until the next change
/// rewrites it. That is the same self-correcting transient the viewer's
/// preference writes already accept, and closing it would mean putting a lock
/// around a file two processes touch seconds apart.
fn persist_view_state(ws: &Workspace) {
    let mut stored = crate::workspace::persistence::load_workspace().unwrap_or_default();
    stored.sessions = ws.view_state();
    crate::workspace::persistence::save_workspace(&stored);
}
