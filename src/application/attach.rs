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
pub(crate) fn run_attach(repo: Vec<std::path::PathBuf>) -> Result<()> {
    let mut client = DaemonClient::connect(&crate::daemon::socket::default_socket_path()?)?;
    // A repository named on the command line is a request like any other: the
    // daemon decides, and the set comes back with it included. Sent before the
    // screen is taken over so a refusal is a plain stderr line, not a notice
    // behind a splash.
    for path in crate::cli::resolve_repo_paths(repo)? {
        client.open_repo(&path)?;
    }

    let cfg = crate::config::load_config()?;
    // Resolved and parsed before the alternate screen so their errors are
    // readable, as in the standalone path.
    let startup_commands = crate::config::resolve_startup_commands(&cfg, &[])?;
    let leader = crate::config::parse_leader(&cfg.input.leader)?;

    // The log anchor cannot follow the tabs, and attached there is no `--repo`
    // to stand in, so it is the working directory the client was started from.
    let anchor = std::env::current_dir()?.to_string_lossy().into_owned();
    let _log_guard = crate::platform::logging::init_logging(&cfg.log, &anchor);
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
    let ctx = ProjectContext {
        cfg: &cfg,
        startup_commands: &startup_commands,
        leader,
    };
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

    if matches!(
        splash_loop(&mut terminal, &ws, cfg.theme.preset_index())?,
        SplashOutcome::Quit
    ) {
        tracing::info!("nightcrow detached during splash");
        return Ok(());
    }
    main_loop(
        &mut terminal,
        &mut ws,
        &ss,
        &ts,
        &cfg,
        &ctx,
        SessionLink::Daemon(Box::new(client)),
    )?;
    // Nothing is persisted here. The daemon owns the workspace file, and a
    // client writing its own view of the tabs on the way out would overwrite
    // what the session actually has.
    tracing::info!("nightcrow detached");
    Ok(())
}
