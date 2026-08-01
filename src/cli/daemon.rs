use anyhow::{Context, Result};
use std::path::{Path, PathBuf};

/// Run the session daemon, respawning it in the background when requested.
pub(crate) fn run_daemon(
    exec: Vec<String>,
    port: Option<u16>,
    bind: Option<String>,
    detach: bool,
) -> Result<()> {
    if detach && !crate::daemon::detach::is_detached_child() {
        let log = daemon_output_path()?;
        let pid = crate::daemon::detach::respawn_in_background(&log)?;
        eprintln!("nightcrow: session running in the background (pid {pid})");
        eprintln!("nightcrow: its output goes to {}", log.display());
        return Ok(());
    }

    // Register before opening repositories or panes so an early signal cannot
    // leave already-spawned children behind.
    let shutdown = crate::platform::signals::ShutdownWatch::register()?;
    let (shutdown_tx, shutdown_rx) =
        std::sync::mpsc::sync_channel::<crate::platform::signals::Shutdown>(1);
    let signal_tx = shutdown_tx.clone();
    std::thread::Builder::new()
        .name("nightcrow-signal-forward".into())
        .spawn(move || {
            if let Ok(signal) = shutdown.wait() {
                let _ = signal_tx.send(signal);
            }
        })
        .context("spawning the signal-forwarding thread")?;

    let mut cfg = crate::config::load_config()?;
    if let Some(port) = port {
        cfg.web_viewer.port = port;
    }
    if let Some(bind) = bind {
        cfg.web_viewer.bind = bind;
    }
    let _log_guard = crate::platform::logging::init_logging(
        &cfg.log,
        &crate::platform::paths::state_dir_anchor(),
    );
    tracing::info!(
        level = cfg.log.level.as_str(),
        rotation = ?cfg.log.rotation,
        prompt_log = cfg.log.prompt_log,
        "logging initialized"
    );

    let config_path = crate::config::config_file_path()?;
    if let Some(password) = crate::config::ensure_web_viewer_password(&mut cfg, &config_path)? {
        eprintln!(
            "nightcrow: generated a web viewer password and saved it to {}:",
            config_path.display()
        );
        eprintln!("  {password}");
    }

    let paths: Vec<String> = crate::workspace::persistence::load_workspace()
        .map(|workspace| workspace.repos)
        .unwrap_or_default()
        .into_iter()
        .filter(|repo| Path::new(repo).is_dir())
        .collect();
    let startup = crate::config::resolve_startup_commands(&cfg, &exec)?;
    let server = crate::web::viewer::server::ViewerServer::start_from_config(
        crate::web::viewer::server::ViewerLaunch {
            viewer: &cfg.web_viewer,
            agent_indicator: &cfg.agent_indicator,
            theme: &cfg.theme,
            shell: &cfg.shell,
            paths: &paths,
            persist: true,
            startup_commands: startup,
            // Retained separately because reload only re-reads configured panes.
            cli_startup: exec,
            plugins: cfg.plugins.clone(),
        },
    )?;
    if paths.is_empty() {
        eprintln!(
            "nightcrow: serving an empty catalog at http://{}/ — open a repository from a client",
            server.addr()
        );
    } else {
        eprintln!(
            "nightcrow: web viewer serving {} repositor{} at http://{}/",
            paths.len(),
            if paths.len() == 1 { "y" } else { "ies" },
            server.addr()
        );
    }

    // Bind the attach socket after the browser listener so a port conflict
    // cannot leave a daemon socket behind.
    let socket_path = crate::daemon::socket::default_socket_path()?;
    let socket = crate::daemon::socket::DaemonSocket::bind(&socket_path)?;
    eprintln!(
        "nightcrow: attach with `nightcrow attach` ({})",
        socket.path().display()
    );
    let listener = socket
        .listener()
        .try_clone()
        .context("cloning the daemon listener")?;
    let session = crate::daemon::serve::start(server.session_state(), shutdown_tx)?;
    std::thread::Builder::new()
        .name("nightcrow-daemon-accept".into())
        .spawn(move || crate::daemon::serve::serve(listener, session))
        .context("spawning the daemon accept thread")?;

    if !server.addr().ip().is_loopback() {
        eprintln!(
            "nightcrow: WARNING bound to {} — repository contents and interactive",
            server.addr().ip()
        );
        eprintln!("nightcrow: shells are reachable from the network over plain HTTP.");
    }
    eprintln!("nightcrow: press Ctrl-C to stop");

    let signal = shutdown_rx
        .recv()
        .context("the shutdown channel closed without a signal")?;
    eprintln!("nightcrow: {} received, stopping", signal.as_str());
    tracing::info!(signal = signal.as_str(), "shutting down");
    server.shutdown();
    // Keep the socket and instance lock until the session has stopped.
    drop(socket);
    Ok(())
}

pub(super) fn daemon_output_path() -> Result<PathBuf> {
    let home = dirs::home_dir().context("cannot determine the home directory")?;
    Ok(home.join(".nightcrow").join("daemon.out"))
}
