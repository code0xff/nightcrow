use anyhow::{Context, Result};
use clap::{Parser, Subcommand};

/// nightcrow — session daemon for agentic coding
///
/// Run with no subcommand to start the session: a git diff viewer and
/// multi-terminal panes, served to a terminal (`nightcrow attach`) and to a
/// browser. Runs in the foreground until interrupted; the session outlives any
/// client that attaches to it.
#[derive(Parser)]
#[command(version, about, long_about = None)]
pub(crate) struct Cli {
    /// Serve this repository. Repeatable, and added to the repositories
    /// remembered from last time rather than replacing them.
    #[arg(short, long)]
    pub(crate) repo: Vec<std::path::PathBuf>,

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
    /// to the daemon, so this starts on whatever it is serving rather than on
    /// the remembered workspace. Leaving does not end the session.
    Attach {
        /// Ask the daemon to open this repository and focus it. Repeatable.
        #[arg(short, long)]
        repo: Vec<std::path::PathBuf>,
    },
}

/// Run the session daemon in the foreground until it is stopped.
///
/// The starting catalog comes from `--repo` plus the remembered workspace —
/// either may be empty, which starts on an empty catalog. From there the
/// clients own the set: attaching terminals and the browser open and close
/// repositories, and the result is written back to the workspace file.
pub(crate) fn run_daemon(
    repos: Vec<std::path::PathBuf>,
    exec: Vec<String>,
    port: Option<u16>,
    bind: Option<String>,
) -> Result<()> {
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
    // `serve` is an explicit request, so the config toggle is not consulted —
    // the user already said what they want by running this.
    cfg.web_viewer.enabled = true;

    let path = crate::config::config_file_path()?;
    if let Some(password) = crate::config::ensure_web_viewer_password(&mut cfg, &path)? {
        eprintln!(
            "nightcrow: generated a web viewer password and saved it to {}:",
            path.display()
        );
        eprintln!("  {password}");
    }

    let mut paths = resolve_serve_repos(&repos)?;
    // Unify with the TUI: restore the previously-open projects so the
    // viewer does not start blank each launch. Explicit --repo comes first and
    // wins; remembered repos that still exist fill in after, de-duplicated.
    if let Some(ws) = crate::workspace::persistence::load_workspace() {
        for repo in ws.repos {
            if std::path::Path::new(&repo).is_dir() && !paths.contains(&repo) {
                paths.push(repo);
            }
        }
    }
    // Resolved before anything is served so a too-many-panes error is a plain
    // stderr line at startup rather than a failure the first client sees.
    let startup = crate::config::resolve_startup_commands(&cfg, &exec)?
        .into_iter()
        .map(|sc| sc.command)
        .collect();
    let server = crate::web::viewer::server::ViewerServer::start_from_config(
        &cfg.web_viewer,
        &cfg.agent_indicator,
        &paths,
        true,
        startup,
    )?;
    if paths.is_empty() {
        // An empty catalog is a legitimate state — the same one the TUI starts
        // in when launched with no repository. The viewer shows its
        // no-repository state and can still be reached; the page's folder
        // picker is the way in from there.
        eprintln!(
            "nightcrow: web viewer serving an empty catalog (no --repo given) at http://{}/",
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
    std::thread::Builder::new()
        .name("nightcrow-daemon-accept".into())
        .spawn(move || crate::daemon::serve::serve(listener, attach_state))
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

/// Canonicalize and de-duplicate the `--repo` list for `serve`.
///
/// Two spellings of one worktree must collapse to one catalog entry, or the
/// browser shows the same repository twice under different ids.
fn resolve_serve_repos(repos: &[std::path::PathBuf]) -> Result<Vec<String>> {
    let mut out: Vec<String> = Vec::new();
    for repo in repos {
        let expanded = crate::platform::paths::expand_tilde(repo);
        if !expanded.exists() {
            anyhow::bail!("no such directory: {}", expanded.display());
        }
        let resolved = crate::git::resolve_repo_path(&expanded)
            .to_string_lossy()
            .into_owned();
        if !out.contains(&resolved) {
            out.push(resolved);
        }
    }
    Ok(out)
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

/// Resolve `--repo` paths to worktree roots, so two spellings of one
/// repository collapse to a single request.
pub(crate) fn resolve_repo_paths(
    repos: Vec<std::path::PathBuf>,
) -> Result<Vec<String>, anyhow::Error> {
    let mut out = Vec::with_capacity(repos.len());
    for p in repos {
        out.push(
            crate::git::resolve_repo_path(crate::platform::paths::expand_tilde(p))
                .to_string_lossy()
                .to_string(),
        );
    }
    Ok(out)
}
