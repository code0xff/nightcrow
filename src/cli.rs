use anyhow::{Context, Result};
use clap::{Parser, Subcommand};

/// nightcrow — TUI for Agentic Coding
///
/// Opens a git diff viewer (top) and multi-terminal panes (bottom)
/// in the current directory.
#[derive(Parser)]
#[command(version, about, long_about = None)]
pub(crate) struct Cli {
    /// Open this repository in a project tab. Repeatable — each --repo adds
    /// a tab. With none, nightcrow starts with no project open.
    #[arg(short, long)]
    pub(crate) repo: Vec<std::path::PathBuf>,

    /// Open a terminal pane running this command at startup. Repeatable;
    /// each --exec adds one pane after any config [[startup_command]] panes.
    #[arg(long = "exec", value_name = "COMMAND")]
    pub(crate) exec: Vec<String>,

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
    /// Serve the web viewer without starting the TUI.
    ///
    /// Runs in the foreground until interrupted. Unlike the TUI's optional
    /// viewer, this needs no terminal — the repositories come from --repo.
    /// --repo is optional: with none, the viewer starts on an empty catalog,
    /// the same state the TUI starts in when launched without a repository.
    Serve {
        /// Repository to serve. Repeatable. Optional — omit to start empty.
        #[arg(short, long)]
        repo: Vec<std::path::PathBuf>,
        /// Override the configured port.
        #[arg(long)]
        port: Option<u16>,
        /// Override the configured bind address. `0.0.0.0` exposes the server
        /// — and the shells it serves — to the whole network over plain HTTP.
        #[arg(long)]
        bind: Option<String>,
    },
}

/// The optional browser surfaces, which start and stop together with the app.
///
/// Grouped because they are always passed as a pair and are the same kind of
/// thing: an independently-failable server the TUI does not depend on.
pub(crate) struct WebSurfaces {
    pub(crate) mirror: Option<crate::web::WebServer>,
    pub(crate) viewer: Option<crate::web::viewer::server::ViewerServer>,
}

/// Start the viewer alongside the TUI when `[web_viewer] enabled` is set.
///
/// Like the mirror, a bind failure only disables the viewer with a warning —
/// the local TUI is the primary interface and must still come up.
pub(crate) fn start_viewer_if_enabled(
    cfg: &mut crate::config::Config,
    repo_paths: &[String],
) -> Result<Option<crate::web::viewer::server::ViewerServer>> {
    if !cfg.web_viewer.enabled {
        return Ok(None);
    }
    let path = crate::config::config_file_path()?;
    if let Some(password) = crate::config::ensure_web_viewer_password(cfg, &path)? {
        eprintln!(
            "nightcrow: generated a web viewer password and saved it to {}:",
            path.display()
        );
        eprintln!("  {password}");
    }
    // The viewer runs the same configured startup terminals as the TUI (in its
    // own, independent PTYs), or one bare shell when none are configured.
    let startup = cfg
        .startup_commands
        .iter()
        .map(|sc| sc.command.clone())
        .collect();
    // Alongside the TUI the viewer does not persist: the TUI owns the workspace
    // file and the catalog already follows its tabs.
    match crate::web::viewer::server::ViewerServer::start_from_config(
        &cfg.web_viewer,
        &cfg.agent_indicator,
        repo_paths,
        false,
        startup,
    ) {
        Ok(server) => {
            eprintln!("nightcrow: web viewer serving at http://{}/", server.addr());
            Ok(Some(server))
        }
        Err(err) => {
            eprintln!("nightcrow: web viewer disabled — {err:#}");
            Ok(None)
        }
    }
}

/// Serve the viewer headlessly until interrupted.
///
/// The starting catalog comes from `--repo` plus the remembered workspace —
/// either may be empty, which starts the viewer on an empty catalog just like
/// the TUI does. From there the browser owns the set: the viewer's own open
/// and close routes add and drop repositories, and `persist` writes the result
/// back to the workspace file since no TUI is doing it.
pub(crate) fn run_serve(
    repos: Vec<std::path::PathBuf>,
    port: Option<u16>,
    bind: Option<String>,
) -> Result<()> {
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
    // Unify with the TUI/mirror: restore the previously-open projects so the
    // viewer does not start blank each launch. Explicit --repo comes first and
    // wins; remembered repos that still exist fill in after, de-duplicated.
    if let Some(ws) = crate::workspace::persistence::load_workspace() {
        for repo in ws.repos {
            if std::path::Path::new(&repo).is_dir() && !paths.contains(&repo) {
                paths.push(repo);
            }
        }
    }
    let startup = cfg
        .startup_commands
        .iter()
        .map(|sc| sc.command.clone())
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

    // The accept loop owns its own threads; park this one until interrupted.
    loop {
        std::thread::park();
    }
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

/// Bootstrap the web login credential and start the mirror server when enabled.
///
/// Runs before the alternate screen so a generated password and any bind error
/// surface as plain stderr. A bind failure disables the web mirror with a
/// warning rather than aborting the whole app — the local TUI still runs.
pub(crate) fn start_web_if_enabled(
    cfg: &mut crate::config::Config,
) -> Result<Option<crate::web::WebServer>> {
    if !cfg.web_mirror.enabled {
        return Ok(None);
    }
    let path = crate::config::config_file_path()?;
    if let Some(password) = crate::config::ensure_web_mirror_password(cfg, &path)? {
        eprintln!(
            "nightcrow web: generated a login password and saved it to {}:",
            path.display()
        );
        eprintln!("  {password}");
    }
    match crate::web::WebServer::start_from_config(&cfg.web_mirror) {
        Ok(server) => {
            eprintln!("nightcrow web: mirror serving at http://{}/", server.addr());
            Ok(Some(server))
        }
        Err(err) => {
            eprintln!("nightcrow web: mirror disabled — {err:#}");
            Ok(None)
        }
    }
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

/// Resolve `--repo` paths to resolved strings, used by `main` before the TUI.
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

/// Pick the log anchor path from the resolved repo list or the cwd.
pub(crate) fn log_anchor_for(repo_paths: &[String]) -> Result<String> {
    match repo_paths.first() {
        Some(path) => Ok(path.clone()),
        None => Ok(std::env::current_dir()
            .context("cannot determine current directory")?
            .to_string_lossy()
            .to_string()),
    }
}
