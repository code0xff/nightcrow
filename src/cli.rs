use anyhow::{Context, Result};
use clap::{Parser, Subcommand};

pub(crate) mod plugin_cmd;

/// nightcrow — session daemon for agentic coding
///
/// Run with no subcommand to start the session: a git diff viewer and
/// multi-terminal panes, served to a terminal (`nightcrow attach`) and to a
/// browser. Runs in the foreground until interrupted; the session outlives any
/// client that attaches to it.
#[derive(Parser)]
#[command(version, about, long_about = None)]
pub(crate) struct Cli {
    /// Open a terminal pane running this command at startup. Repeatable;
    /// each --exec adds one pane after any config [[startup_command]] panes.
    #[arg(long = "exec", value_name = "COMMAND")]
    pub(crate) exec: Vec<String>,

    /// Override the configured browser port.
    #[arg(long)]
    pub(crate) port: Option<u16>,

    /// Override the configured bind address. `0.0.0.0` exposes the server —
    /// and the shells it serves — to the whole network over plain HTTP.
    #[arg(long)]
    pub(crate) bind: Option<String>,

    /// Run the session in the background and return to the shell.
    ///
    /// It gets its own session, so closing this terminal does not stop it, and
    /// its output goes to ~/.nightcrow/daemon.out. A service manager should
    /// start nightcrow *without* this — backgrounding is what it does itself.
    #[arg(short, long)]
    pub(crate) detach: bool,

    #[command(subcommand)]
    pub(crate) command: Option<Commands>,
}

#[derive(Subcommand)]
pub(crate) enum Commands {
    /// Write a starter config file to ~/.nightcrow/config.toml
    Init {
        /// Overwrite the config file if it already exists
        #[arg(long)]
        force: bool,
    },
    /// Attach the TUI to a running nightcrow daemon.
    ///
    /// The session — which repositories are open, and in what order — belongs
    /// to the daemon, so this starts on whatever it is serving. Repositories
    /// are opened from inside, with the leader chord's open dialog or the
    /// browser's folder picker. Leaving does not end the session.
    Attach,
    /// Manage plugin executables in ~/.nightcrow/plugins.
    ///
    /// Installing one only puts the binary in place; it stays inert until
    /// config.toml declares it and a startup pane opts in by name.
    Plugin {
        #[command(subcommand)]
        command: plugin_cmd::PluginCommands,
    },
}

/// Run the session daemon in the foreground until it is stopped.
///
/// The starting catalog is whatever was open last time, which may be nothing —
/// an empty catalog is a normal state, and the way in from there is a client:
/// the browser's folder picker or an attached TUI's open dialog. There is no
/// flag for it. Repositories are opened from inside the session, which is the
/// only place that can open one *and* have every other client see it.
pub(crate) fn run_daemon(
    exec: Vec<String>,
    port: Option<u16>,
    bind: Option<String>,
    detach: bool,
) -> Result<()> {
    // Before the shutdown handlers and before anything is bound: the
    // foreground copy hands the whole job over and exits, so it must not have
    // taken the socket or the instance lock first.
    if detach && !crate::daemon::detach::is_detached_child() {
        let log = daemon_output_path()?;
        let pid = crate::daemon::detach::respawn_in_background(&log)?;
        eprintln!("nightcrow: session running in the background (pid {pid})");
        eprintln!("nightcrow: its output goes to {}", log.display());
        return Ok(());
    }

    // Before anything that can be interrupted. Opening repositories and running
    // the startup shells takes long enough for a stop signal to land in the
    // middle, and until the handlers exist such a signal kills the process
    // outright — leaving the shells it had already spawned behind.
    let shutdown = crate::platform::signals::ShutdownWatch::register()?;

    let mut cfg = crate::config::load_config()?;
    if let Some(port) = port {
        cfg.web_viewer.port = port;
    }
    if let Some(bind) = bind {
        cfg.web_viewer.bind = bind;
    }
    // Logging comes up before anything is served, so a failure while opening
    // repositories has somewhere to go. Anchored at the nightcrow directory
    // rather than a repository: the daemon has no one repository, and the
    // relative default (`.nightcrow/logs`) would otherwise land wherever the
    // process happened to be started.
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

    let path = crate::config::config_file_path()?;
    if let Some(password) = crate::config::ensure_web_viewer_password(&mut cfg, &path)? {
        eprintln!(
            "nightcrow: generated a web viewer password and saved it to {}:",
            path.display()
        );
        eprintln!("  {password}");
    }

    // Repositories that have been moved or deleted since are dropped rather
    // than served as broken tabs.
    let paths: Vec<String> = crate::workspace::persistence::load_workspace()
        .map(|ws| ws.repos)
        .unwrap_or_default()
        .into_iter()
        .filter(|repo| std::path::Path::new(repo).is_dir())
        .collect();
    // Resolved before anything is served so a too-many-panes error is a plain
    // stderr line at startup rather than a failure the first client sees.
    // Names and all: the session is what opens these panes now, so it is what
    // has to know what they are called. Dropping the configured name here left
    // every startup pane titled "shell 1" in every client.
    let startup = crate::config::resolve_startup_commands(&cfg, &exec)?;
    let server = crate::web::viewer::server::ViewerServer::start_from_config(
        &cfg.web_viewer,
        &cfg.agent_indicator,
        &cfg.theme,
        &cfg.shell,
        &paths,
        true,
        startup,
        // Remembered rather than folded away: a config reload re-reads the file's
        // `[[startup_command]]` table, and these panes are not in the file.
        exec,
        cfg.plugins.clone(),
    )?;
    if paths.is_empty() {
        // An empty catalog is a legitimate state — the same one the TUI starts
        // in when launched with no repository. The viewer shows its
        // no-repository state and can still be reached; the page's folder
        // picker is the way in from there.
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
    // The attach socket comes up after the browser listener so a port conflict
    // is reported before a socket file exists to clean up. A failure here does
    // abort: unlike the viewer beside the TUI, this *is* the session, and
    // running on with no way to attach would look like a silent success.
    let socket_path = crate::daemon::socket::default_socket_path()?;
    let socket = crate::daemon::socket::DaemonSocket::bind(&socket_path)?;
    eprintln!(
        "nightcrow: attach with `nightcrow attach` ({})",
        socket.path().display()
    );
    // The accept loop gets a clone; `socket` stays here so that returning from
    // this function unlinks it and releases the instance lock. Parked in the
    // accept thread it would be freed by process exit, which runs no destructor
    // — and the socket file would outlive every clean shutdown.
    let listener = socket
        .listener()
        .try_clone()
        .context("cloning the daemon listener")?;
    let attach_state = server.state();
    // Before the accept thread, so a session that cannot watch itself is
    // reported here — where it can still fail — rather than by an accept loop
    // nobody is reading the result of.
    let session = crate::daemon::serve::start(attach_state)?;
    std::thread::Builder::new()
        .name("nightcrow-daemon-accept".into())
        .spawn(move || crate::daemon::serve::serve(listener, session))
        .context("spawning the daemon accept thread")?;

    if !server.addr().ip().is_loopback() {
        // Worth saying out loud: this is not the default, it carries shells,
        // and there is no TLS to fall back on.
        eprintln!(
            "nightcrow: WARNING bound to {} — repository contents and interactive",
            server.addr().ip()
        );
        eprintln!("nightcrow: shells are reachable from the network over plain HTTP.");
    }
    eprintln!("nightcrow: press Ctrl-C to stop");

    // The accept loop owns its own threads, so this one only waits for the
    // stop signal. It must run the shutdown rather than let the process die
    // under the signal's default disposition: the server owns child shells,
    // and only `shutdown` walks the catalog to kill them.
    let signal = shutdown.wait()?;
    eprintln!("nightcrow: {} received, stopping", signal.as_str());
    tracing::info!(signal = signal.as_str(), "shutting down");
    server.shutdown();
    // Explicit so the order is visible: the socket file goes away and the
    // instance lock is released only after the session has stopped, so nothing
    // can attach to a daemon that is already tearing down its terminals.
    drop(socket);
    Ok(())
}

/// Where a backgrounded session writes what it would have printed.
///
/// Beside the socket and the workspace file rather than in the log directory:
/// this is the startup banner and any bind error, which a person goes looking
/// for by hand, not the rotated tracing log.
fn daemon_output_path() -> Result<std::path::PathBuf> {
    let home = dirs::home_dir().context("cannot determine the home directory")?;
    Ok(home.join(".nightcrow").join("daemon.out"))
}

pub(crate) fn run_init(force: bool) -> Result<()> {
    match crate::config::init_config(force)? {
        crate::config::InitOutcome::Created(path) => {
            println!("Created starter config at {}", path.display());
            println!("Edit it to reserve startup commands, panel layout, theme, and more.");
        }
        crate::config::InitOutcome::AlreadyExists(path) => {
            println!(
                "Config already exists at {} — left untouched (pass --force to overwrite).",
                path.display()
            );
        }
    }
    Ok(())
}
