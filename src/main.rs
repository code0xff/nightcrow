mod app;
mod backend;
mod config;
mod git;
mod input;
mod logging;
mod runtime;
mod session;
#[cfg(test)]
mod test_util;
mod ui;
mod util;
mod web;
mod workspace;

use anyhow::{Context, Result};
use app::{App, DiffPaneView, Focus, ViewMode};
use clap::{Parser, Subcommand};
use crossterm::event::{
    DisableBracketedPaste, DisableMouseCapture, EnableBracketedPaste, EnableMouseCapture, KeyCode,
    KeyEvent, KeyEventKind, KeyModifiers, MouseEvent, MouseEventKind,
};
use crossterm::{
    event::{self, Event},
    execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use input::{
    Action, encode_key, map_key, prefix_action, prefix_action_fullscreen, vim_navigation_action,
};
use ratatui::{Terminal, backend::CrosstermBackend, layout::Rect};
use runtime::terminal::{SCROLL_LINE_STEP, WHEEL_LINES_PER_NOTCH};
use std::{io, time::Duration};
use syntect::highlighting::ThemeSet;
use syntect::parsing::SyntaxSet;
use workspace::Workspace;

/// nightcrow — TUI for Agentic Coding
///
/// Opens a git diff viewer (top) and multi-terminal panes (bottom)
/// in the current directory.
#[derive(Parser)]
#[command(version, about, long_about = None)]
struct Cli {
    /// Open this repository in a project tab. Repeatable — each --repo adds
    /// a tab. With none, nightcrow starts with no project open.
    #[arg(short, long)]
    repo: Vec<std::path::PathBuf>,

    /// Open a terminal pane running this command at startup. Repeatable;
    /// each --exec adds one pane after any config [[startup_command]] panes.
    #[arg(long = "exec", value_name = "COMMAND")]
    exec: Vec<String>,

    #[command(subcommand)]
    command: Option<Commands>,
}

#[derive(Subcommand)]
enum Commands {
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

fn main() -> Result<()> {
    let cli = Cli::parse();

    // Subcommands run to completion and exit before any TUI setup, so their
    // output stays on the normal terminal rather than flashing behind the
    // alternate screen.
    match cli.command {
        Some(Commands::Init { force }) => return run_init(force),
        Some(Commands::Serve { repo, port, bind }) => return run_serve(repo, port, bind),
        None => {}
    }

    let mut cfg = config::load_config()?;
    // Bootstrap the web login credential and start the server before the
    // alternate screen, so a freshly generated password and any bind error
    // print as plain, copyable stderr text rather than flashing behind the TUI.
    let web_server = start_web_if_enabled(&mut cfg)?;
    // Resolve before entering the alternate screen so a too-many-panes error
    // surfaces as plain stderr text rather than a flash behind the TUI.
    let startup_commands = config::resolve_startup_commands(&cfg, &cli.exec)?;
    // Parse the leader before the alternate screen too, so a malformed
    // `[input] leader` is reported as plain stderr. `load_config` already
    // validated it; re-parsing keeps the KeyEvent local to the app setup.
    let leader = config::parse_leader(&cfg.input.leader)?;

    let repo_paths: Vec<String> = cli
        .repo
        .into_iter()
        .map(|p| {
            git::resolve_repo_path(util::expand_tilde(p))
                .to_string_lossy()
                .to_string()
        })
        .collect();

    // The viewer needs the resolved repository list, so it starts after it is
    // built — still before the alternate screen, so its generated password and
    // any bind error stay readable on stderr.
    let surfaces = WebSurfaces {
        mirror: web_server,
        viewer: start_viewer_if_enabled(&mut cfg, &repo_paths)?,
    };

    // Logs live under a repo by default, so with no project the first one
    // named on the command line stands in; with none at all, the working
    // directory does. A log path cannot follow the active tab — the file is
    // opened once, at startup.
    let log_anchor = match repo_paths.first() {
        Some(path) => path.clone(),
        None => std::env::current_dir()
            .context("cannot determine current directory")?
            .to_string_lossy()
            .to_string(),
    };
    let _log_guard = logging::init_logging(&cfg.log, &log_anchor);

    tracing::info!(
        level = cfg.log.level.as_str(),
        rotation = ?cfg.log.rotation,
        prompt_log = cfg.log.prompt_log,
        "logging initialized"
    );

    let _guard = TerminalGuard::enter(cfg.mouse.enabled)?;

    let original_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let _ = disable_raw_mode();
        let _ = execute!(io::stdout(), DisableMouseCapture, LeaveAlternateScreen);
        original_hook(info);
    }));

    let backend = CrosstermBackend::new(io::stdout());
    let mut terminal = Terminal::new(backend)?;

    run(
        &mut terminal,
        repo_paths,
        cfg,
        startup_commands,
        leader,
        surfaces,
    )
}

/// The optional browser surfaces, which start and stop together with the app.
///
/// Grouped because they are always passed as a pair and are the same kind of
/// thing: an independently-failable server the TUI does not depend on.
struct WebSurfaces {
    mirror: Option<web::WebServer>,
    viewer: Option<web::viewer::server::ViewerServer>,
}

/// Start the viewer alongside the TUI when `[web_viewer] enabled` is set.
///
/// Like the mirror, a bind failure only disables the viewer with a warning —
/// the local TUI is the primary interface and must still come up.
fn start_viewer_if_enabled(
    cfg: &mut config::Config,
    repo_paths: &[String],
) -> Result<Option<web::viewer::server::ViewerServer>> {
    if !cfg.web_viewer.enabled {
        return Ok(None);
    }
    let path = config::config_file_path()?;
    if let Some(password) = config::ensure_web_viewer_password(cfg, &path)? {
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
    match web::viewer::server::ViewerServer::start_from_config(
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
fn run_serve(
    repos: Vec<std::path::PathBuf>,
    port: Option<u16>,
    bind: Option<String>,
) -> Result<()> {
    let mut cfg = config::load_config()?;
    if let Some(port) = port {
        cfg.web_viewer.port = port;
    }
    if let Some(bind) = bind {
        cfg.web_viewer.bind = bind;
    }
    // `serve` is an explicit request, so the config toggle is not consulted —
    // the user already said what they want by running this.
    cfg.web_viewer.enabled = true;

    let path = config::config_file_path()?;
    if let Some(password) = config::ensure_web_viewer_password(&mut cfg, &path)? {
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
    if let Some(ws) = session::load_workspace() {
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
    let server = web::viewer::server::ViewerServer::start_from_config(
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
        let expanded = util::expand_tilde(repo);
        if !expanded.exists() {
            anyhow::bail!("no such directory: {}", expanded.display());
        }
        let resolved = git::resolve_repo_path(&expanded)
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
fn start_web_if_enabled(cfg: &mut config::Config) -> Result<Option<web::WebServer>> {
    if !cfg.web_mirror.enabled {
        return Ok(None);
    }
    let path = config::config_file_path()?;
    if let Some(password) = config::ensure_web_mirror_password(cfg, &path)? {
        eprintln!(
            "nightcrow web: generated a login password and saved it to {}:",
            path.display()
        );
        eprintln!("  {password}");
    }
    match web::WebServer::start_from_config(&cfg.web_mirror) {
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

fn run_init(force: bool) -> Result<()> {
    match config::init_config(force)? {
        config::InitOutcome::Created(path) => {
            println!("Created starter config at {}", path.display());
            println!("Edit it to reserve startup commands, panel layout, theme, and more.");
        }
        config::InitOutcome::AlreadyExists(path) => {
            println!(
                "Config already exists at {} — left untouched (pass --force to overwrite).",
                path.display()
            );
        }
    }
    Ok(())
}

struct TerminalGuard;

impl TerminalGuard {
    fn enter(mouse: bool) -> Result<Self> {
        enable_raw_mode()?;
        // EnableBracketedPaste makes crossterm surface paste as
        // `Event::Paste(String)` instead of a flood of `Event::Key` chars —
        // the latter would each be filtered as control chars by the search
        // handler and silently drop newlines.
        if let Err(err) = execute!(io::stdout(), EnterAlternateScreen, EnableBracketedPaste) {
            let _ = disable_raw_mode();
            return Err(err.into());
        }
        // Mouse capture is config-gated (`[mouse] enabled`): while captured,
        // the outer terminal only selects text with Shift held, so users who
        // prefer plain-drag selection can hand the mouse back entirely.
        if mouse && let Err(err) = execute!(io::stdout(), EnableMouseCapture) {
            // The enable may have partially reached the terminal even though
            // the call errored (e.g. the write landed but a later flush
            // failed), and no TerminalGuard exists yet to undo it on drop —
            // send the disable explicitly; it is harmless when capture never
            // took effect.
            let _ = execute!(
                io::stdout(),
                DisableMouseCapture,
                DisableBracketedPaste,
                LeaveAlternateScreen
            );
            let _ = disable_raw_mode();
            return Err(err.into());
        }

        Ok(Self)
    }
}

impl Drop for TerminalGuard {
    fn drop(&mut self) {
        // DisableMouseCapture is unconditional: it merely writes the reset
        // sequences, which are harmless when capture was never enabled.
        let _ = execute!(
            io::stdout(),
            DisableMouseCapture,
            DisableBracketedPaste,
            LeaveAlternateScreen
        );
        let _ = disable_raw_mode();
    }
}

#[derive(Debug, PartialEq, Eq)]
enum KeyOutcome {
    Continue,
    /// Force a full repaint on the next frame. Used by the `<prefix> r` redraw
    /// chord to wipe stray glyphs left behind when a PTY child writes cells
    /// ratatui's diff renderer doesn't track.
    Redraw,
    Quit,
    /// The key asked for something only the workspace can do. The handlers
    /// take `&mut App` — one project — so they cannot reach the tab list;
    /// they name the intent here and `main_loop` carries it out.
    Project(ProjectRequest),
}

/// A workspace-level action requested by a key or click.
#[derive(Debug, PartialEq, Eq)]
enum ProjectRequest {
    /// Focus the tab at this index. Out-of-range indices are inert.
    Switch(usize),
    /// Close the active tab. Refused when it is the only one.
    Close,
    /// Open this resolved repo path as a tab, or focus the tab already on it.
    Open(String),
    /// Raise the open-repo dialog. It lives on the workspace, so a handler
    /// holding one project cannot open it directly.
    OpenDialog,
}

fn run(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    repo_paths: Vec<String>,
    cfg: config::Config,
    startup_commands: Vec<config::StartupCommand>,
    leader: KeyEvent,
    surfaces: WebSurfaces,
) -> Result<()> {
    // syntect's bundled defaults omit TypeScript/TSX/TOML/YAML; two-face
    // supplies bat's expanded syntax set (newline variant matches the diff /
    // file-view highlighters, which feed whole lines including trailing \n).
    let ss = two_face::syntax::extra_newlines();
    let ts = ThemeSet::load_defaults();
    let ctx = ProjectContext {
        cfg: &cfg,
        startup_commands: &startup_commands,
        leader,
    };
    let mut ws = Workspace::new(leader);
    // Named repos win over the remembered ones: `--repo X` says which repo to
    // work in, and restoring extra tabs beside it would be surprising. With no
    // argument, the last set of tabs comes back — and since quitting with none
    // open records exactly that, an empty screen stays reachable without a
    // dedicated flag.
    let stored = session::load_workspace();
    // The same file carries the tab list and every repo's view state, so the
    // remembered half is seeded even when `--repo` overrides the tab list.
    if let Some(state) = stored.as_ref() {
        ws.set_remembered(state.sessions.clone());
    }
    let restored = repo_paths.is_empty().then_some(stored).flatten();
    // Tracked by path, not index: skipping a missing repo compacts the list,
    // so the saved index would then name a different tab.
    let restored_active_repo = restored
        .as_ref()
        .and_then(|state| state.repos.get(state.active).cloned());
    let (repo_paths, from_restore) = match restored {
        Some(state) => (state.repos, true),
        None => (repo_paths, false),
    };
    // Restored repos can have been moved or deleted since; opening a tab on a
    // path that is gone would show a broken project rather than nothing.
    let mut missing = 0usize;
    // Every repo opens a tab. Repeats are skipped for the same reason the
    // dialog focuses an open repo instead of opening it twice: two projects on
    // one workdir would run duplicate snapshot workers and write the same
    // session file. Past `MAX_PROJECTS` the extras are dropped rather than
    // silently replacing earlier ones; the notice says so.
    let mut overflowed = false;
    for path in &repo_paths {
        // Only remembered repos are filtered. An explicit `--repo` that does
        // not exist still opens, so the git error says so — silently dropping
        // it would look like the argument was ignored.
        if from_restore && !std::path::Path::new(path).is_dir() {
            missing += 1;
            continue;
        }
        if ws.index_of_repo(path).is_some() {
            continue;
        }
        // Checked before building, like the dialog path: `init_app` spawns a
        // PTY and runs the configured startup commands, so constructing a
        // project only to have `add` refuse it would leave those side effects
        // behind for a tab that never opens.
        if ws.is_full() {
            overflowed = true;
            break;
        }
        let saved = ws.session_for(path).cloned();
        ws.add(init_app(path, &cfg, &startup_commands, leader, saved));
    }
    // Land on the tab that was in front, found by path so a skipped repo
    // earlier in the list cannot shift the choice onto its neighbour.
    let active_idx = restored_active_repo
        .and_then(|repo| ws.index_of_repo(&repo))
        .unwrap_or(0);
    ws.switch(active_idx);
    // Raised after the switch: notices are project-scoped, so reporting this
    // before would leave it on a tab the user never sees, hiding the only sign
    // that something was dropped.
    if overflowed {
        ws.raise_notice(
            app::NoticeKind::Project,
            format!("cannot open more than {} projects", workspace::MAX_PROJECTS),
        );
    } else if missing > 0 {
        ws.raise_notice(
            app::NoticeKind::Project,
            format!("{missing} remembered repo(s) no longer exist"),
        );
    }

    if matches!(
        splash_loop(terminal, &ws, cfg.theme.preset_index())?,
        SplashOutcome::Quit
    ) {
        tracing::info!("nightcrow stopped during splash");
        return Ok(());
    }
    main_loop(terminal, &mut ws, &ss, &ts, &cfg, &ctx, surfaces)?;

    // Every open project gets its session written, not just the active one:
    // sessions are stored per repo (`<repo>/.nightcrow/session.json`), so a
    // background project's pane/focus state would otherwise be lost purely
    // because the user happened to quit from another tab.
    session::save_workspace(&ws.to_persisted());
    tracing::info!("nightcrow stopped");
    Ok(())
}

/// Everything a project needs beyond its repo path.
///
/// Threaded to the input handlers rather than stored on `Workspace` so the
/// workspace stays a pure state container: opening a tab is the only thing
/// that needs the config, and it borrows it for the duration of one keypress.
struct ProjectContext<'a> {
    cfg: &'a config::Config,
    startup_commands: &'a [config::StartupCommand],
    leader: KeyEvent,
}

/// Carry out a workspace-level request produced by a key or click.
///
/// Refusals land on the notice row rather than being dropped: a keypress that
/// appears to do nothing reads as a bug.
fn apply_project_request(ws: &mut Workspace, ctx: &ProjectContext, request: ProjectRequest) {
    match request {
        ProjectRequest::Switch(idx) => ws.switch(idx),
        ProjectRequest::OpenDialog => ws.start_repo_input(),
        ProjectRequest::Close => {
            // `close_active` carries the project's view state into the
            // remembered set; writing here means a crash later cannot lose it.
            if ws.close_active() {
                session::save_workspace(&ws.to_persisted());
            }
        }
        ProjectRequest::Open(repo_path) => {
            // Focus rather than duplicate: two tabs on one workdir would show
            // identical git state while racing each other's snapshot workers.
            if let Some(idx) = ws.index_of_repo(&repo_path) {
                ws.switch(idx);
                return;
            }
            // Checked before building: `init_app` spawns a PTY backend and runs
            // the configured startup commands, so constructing a project only
            // to have `add` refuse it would leave those processes behind.
            if ws.is_full() {
                ws.raise_notice(
                    app::NoticeKind::Project,
                    format!("cannot open more than {} projects", workspace::MAX_PROJECTS),
                );
                return;
            }
            let saved = ws.session_for(&repo_path).cloned();
            let project = init_app(&repo_path, ctx.cfg, ctx.startup_commands, ctx.leader, saved);
            ws.add(project);
        }
    }
}

/// Carry out a handler's outcome. Returns `true` when the app should quit.
fn apply_outcome(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    ws: &mut Workspace,
    ctx: &ProjectContext,
    outcome: KeyOutcome,
) -> Result<bool> {
    match outcome {
        KeyOutcome::Quit => return Ok(true),
        KeyOutcome::Redraw => terminal.clear()?,
        KeyOutcome::Continue => {}
        KeyOutcome::Project(request) => apply_project_request(ws, ctx, request),
    }
    Ok(false)
}

fn init_app(
    repo_path: &str,
    cfg: &config::Config,
    startup_commands: &[config::StartupCommand],
    leader: KeyEvent,
    saved_session: Option<crate::session::SessionState>,
) -> App {
    let mut app = App::new(
        repo_path.to_string(),
        cfg.log.prompt_log,
        startup_commands,
        leader,
    );
    app.set_accent_index(cfg.theme.preset_index());
    app.cfg_agent_indicator = cfg.agent_indicator.clone();
    app.cfg_tree = cfg.tree.clone();
    app.mouse_enabled = cfg.mouse.enabled;
    if cfg.tree.live_watch {
        app.tree_watch = crate::runtime::tree_watch::TreeWatcher::new();
    }
    app.pagination.page_size = cfg.log.commit_log_page_size;
    app.pagination.prefetch_threshold = cfg.log.commit_log_prefetch_threshold;
    if let Some(state) = saved_session {
        // Applied up front rather than on the first snapshot: only the Status
        // selection needs the changed-file list, and it waits in
        // `pending_selection` (see `App::restore_session`). Restoring here also
        // keeps the fresh-launch terminal focus set by `ensure_initial_terminal`
        // from drawing — or routing keystrokes — over the saved focus.
        app.restore_session(&state);
    }
    app
}

enum SplashOutcome {
    Enter,
    Quit,
}

fn splash_loop(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    ws: &Workspace,
    fallback_accent: usize,
) -> Result<SplashOutcome> {
    let splash = ui::splash::SplashState::new();
    // With no project open there is no restored accent to honour, so the
    // configured preset stands in.
    let accent = ws
        .active()
        .map(|p| p.current_accent())
        .unwrap_or_else(|| config::Accent::from_index(fallback_accent).color());
    loop {
        terminal.draw(|frame| {
            ui::splash::draw(frame, &splash, accent);
        })?;
        if splash.is_done() {
            break;
        }
        if event::poll(Duration::from_millis(16))? {
            match event::read()? {
                // Honour Esc so the user can abort during the splash instead
                // of being forced to wait for it to clear and quit from the
                // main view. (Leader-based quit needs a two-key sequence, so
                // it isn't recognised on the one-shot splash screen.) Any
                // other key dismisses the splash.
                Event::Key(k) if k.kind == KeyEventKind::Press => {
                    if k.code == KeyCode::Esc {
                        return Ok(SplashOutcome::Quit);
                    }
                    break;
                }
                Event::Resize(_, _) => terminal.clear()?,
                _ => {}
            }
        }
    }
    terminal.clear()?;
    Ok(SplashOutcome::Enter)
}

fn main_loop(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    ws: &mut Workspace,
    ss: &SyntaxSet,
    ts: &ThemeSet,
    cfg: &config::Config,
    ctx: &ProjectContext,
    surfaces: WebSurfaces,
) -> Result<()> {
    let WebSurfaces {
        mirror: mut web_server,
        viewer,
    } = surfaces;
    // Signature of the repository set last handed to the viewer. The catalog
    // only needs updating when a tab opens or closes, not every frame.
    let mut served_repos: Vec<String> = Vec::new();
    loop {
        if let Some(viewer) = viewer.as_ref() {
            let current: Vec<String> = ws.projects().iter().map(|p| p.repo_path.clone()).collect();
            if current != served_repos {
                viewer.set_repos(&current);
                served_repos = current;
            }
        }
        // Every project drains its queues, not just the visible one: the
        // snapshot worker and PTY reader produce into unbounded channels
        // regardless of which tab is on screen, so skipping the background
        // ones would let them grow until the user switched back.
        //
        // Only the active project *applies* its snapshot, though. That runs a
        // full `refresh_diff`, and doing it for every open project would put
        // several repositories' git diffs on the UI thread every tick. A
        // background snapshot waits in `pending_snapshot` until its tab is
        // shown (see `App::drain_snapshot`).
        let active = ws.active_index();
        for (i, project) in ws.projects_mut().iter_mut().enumerate() {
            if i == active {
                project.poll_snapshot();
                // Applying a commit-log page can trigger a further prefetch and
                // load a commit diff synchronously, so it stays with the
                // snapshot as active-only work. A hidden project's in-flight
                // fetch is capped at one by `CommitLogPagination`, so its reply
                // can wait in the channel without growing.
                project.poll_commit_log_page_fetch();
            } else {
                project.drain_snapshot();
            }
            // Both are cheap drains that must run everywhere: the tree watcher
            // to keep OS filesystem events from piling up, the terminal to
            // consume PTY output before the pipe fills and blocks the child.
            // Acting on a watcher event rereads directories and previews a
            // file, so like the snapshot that is active-only; a hidden project
            // records the event and refreshes when its tab comes forward.
            if i == active {
                project.poll_tree_watcher();
            } else {
                project.drain_tree_watcher();
            }
            project.poll_terminal();
        }

        let size = terminal.size()?;
        let screen = Rect::new(0, 0, size.width, size.height);
        if let Some(app) = ws.active() {
            let layouts: Vec<(backend::PaneId, u16, u16)> =
                ui::terminal_content_areas(app, screen, &cfg.layout)
                    .into_iter()
                    .map(|(id, area)| (id, area.height, area.width))
                    .collect();
            let app = ws.active_mut().expect("active project checked above");
            app.terminal.resize_visible_panes(&layouts);
            app.terminal.sync_scroll();
        }

        // Collected before the mutable borrow of the active project, since the
        // tab row names every project while the body renders only one. Bounded
        // by `MAX_PROJECTS`, so the per-frame clone is a handful of short
        // strings.
        let tab_paths: Vec<String> = ws.projects().iter().map(|p| p.repo_path.clone()).collect();
        let active_tab = ws.active_index();
        let empty_notice = ws.empty_notice().cloned();
        let prefix_armed = ws.prefix_armed();
        let fallback_accent = config::Accent::from_index(cfg.theme.preset_index()).color();

        let (app_opt, repo_input) = ws.render_parts();
        let tabs = ui::Chrome {
            repo_paths: &tab_paths,
            active: active_tab,
            repo_input,
        };
        let accent = app_opt
            .as_ref()
            .map(|app| app.current_accent())
            .unwrap_or(fallback_accent);
        let mut cursor = None;
        let completed = terminal.draw(|frame| {
            cursor = match app_opt {
                Some(app) => ui::draw(frame, app, tabs, ss, ts, &cfg.layout, accent),
                None => {
                    ui::draw_empty(
                        frame,
                        tabs,
                        empty_notice.as_ref(),
                        ctx.leader,
                        prefix_armed,
                        cfg.mouse.enabled,
                        accent,
                    );
                    None
                }
            };
        })?;

        // Mirror the freshly composited frame to any connected browsers. Use the
        // buffer returned by `draw` — after it swaps buffers, `current_buffer_mut`
        // points at the next (reset) frame, not the one just rendered. The local
        // terminal stays the authority for the grid size; the web view renders
        // the exact same cells.
        if let Some(server) = web_server.as_mut() {
            server.broadcast(completed.buffer, cursor);
        }

        // `tabs` above borrows the workspace for the draw; input needs it
        // mutably, so rebuild the same view over a snapshot of the dialog.
        // Only the buffer is copied, and only on frames that see an event.
        let repo_input = ws.repo_input.clone();
        let tabs = ui::Chrome {
            repo_paths: &tab_paths,
            active: active_tab,
            repo_input: &repo_input,
        };

        // 16 ms ≈ 60 fps. The previous 50 ms tick noticeably lagged PTY echo
        // on every keystroke (typing felt sticky). `event::poll` performs an
        // OS-level wait when nothing is happening, so the higher cap doesn't
        // burn CPU at idle.
        if event::poll(Duration::from_millis(16))? {
            match event::read()? {
                // Ratatui's next draw will pick up the new size from
                // `Frame::area()`. An explicit clear() here only adds a
                // visible flash on resize without improving correctness.
                Event::Resize(_, _) => {}
                Event::Key(key) => {
                    let outcome = dispatch_key(ws, key);
                    if apply_outcome(terminal, ws, ctx, outcome)? {
                        return Ok(());
                    }
                }
                Event::Paste(text) => dispatch_paste(ws, &text),
                Event::Mouse(mouse) => {
                    let screen = Rect::new(0, 0, size.width, size.height);
                    let outcome =
                        dispatch_mouse(ws, tabs, mouse, screen, &cfg.layout, cfg.mouse.enabled);
                    if apply_outcome(terminal, ws, ctx, outcome)? {
                        return Ok(());
                    }
                }
                _ => {}
            }
        }

        // Browser input runs through the exact same handlers as local input, so
        // a web action can never diverge from the equivalent local keypress.
        if let Some(server) = web_server.as_ref() {
            let screen = Rect::new(0, 0, size.width, size.height);
            for event in server.drain_input() {
                // Rebuilt per event, not reused from the frame: an earlier
                // event in this batch (or the local input above) may have
                // opened, closed, or switched a project, and a tab hit-test
                // against the stale row would select the wrong one.
                let tab_paths: Vec<String> =
                    ws.projects().iter().map(|p| p.repo_path.clone()).collect();
                let active_tab = ws.active_index();
                let repo_input = ws.repo_input.clone();
                let tabs = ui::Chrome {
                    repo_paths: &tab_paths,
                    active: active_tab,
                    repo_input: &repo_input,
                };
                let outcome =
                    dispatch_web_event(ws, tabs, event, screen, &cfg.layout, cfg.mouse.enabled);
                if apply_outcome(terminal, ws, ctx, outcome)? {
                    return Ok(());
                }
            }
        }
    }
}

/// Route a decoded browser input event through the same handlers as local
/// input. Keeps web and terminal control behaviourally identical.
fn dispatch_web_event(
    ws: &mut Workspace,
    tabs: ui::Chrome<'_>,
    event: web::protocol::WebInputEvent,
    screen: Rect,
    layout: &config::LayoutConfig,
    mouse_enabled: bool,
) -> KeyOutcome {
    use web::protocol::WebInputEvent;
    match event {
        WebInputEvent::Key(key) => dispatch_key(ws, key),
        WebInputEvent::Mouse(mouse) => {
            dispatch_mouse(ws, tabs, mouse, screen, layout, mouse_enabled)
        }
        WebInputEvent::Paste(text) => {
            dispatch_paste(ws, &text);
            KeyOutcome::Continue
        }
    }
}

/// Route one mouse event. The project tab row is the only target that exists
/// with no project open, so it is resolved before the per-project handler.
fn dispatch_mouse(
    ws: &mut Workspace,
    tabs: ui::Chrome<'_>,
    mouse: MouseEvent,
    screen: Rect,
    layout: &config::LayoutConfig,
    mouse_enabled: bool,
) -> KeyOutcome {
    let ws_leader = ws.leader();
    // A release must reach the pane whose press it pairs with, even when the
    // dialog opened in between: no drag reports are forwarded, so that program
    // cannot track the pointer itself, and a swallowed release leaves
    // `pending_mouse_press` set for a later unrelated release to match.
    // `handle_mouse` resolves releases before its own modal guard for exactly
    // this reason, so the dialog must not swallow them ahead of it either.
    let is_release = matches!(mouse.kind, MouseEventKind::Up(_));
    if ws.repo_input.active && !is_release {
        return KeyOutcome::Continue;
    }
    match ws.active_mut() {
        Some(app) => handle_mouse(app, tabs, mouse, screen, layout),
        None => {
            let MouseEventKind::Down(crossterm::event::MouseButton::Left) = mouse.kind else {
                return KeyOutcome::Continue;
            };
            if let Some(idx) = ui::project_tab_at(tabs, screen, mouse.column, mouse.row) {
                return KeyOutcome::Project(ProjectRequest::Switch(idx));
            }
            // The open hint is the one action the empty screen offers, so a
            // click on it does what its key does.
            let leader_label = app::leader_label_of(ws_leader);
            let armed = ws.prefix_armed();
            match ui::empty_hint_click_at(
                screen,
                &leader_label,
                armed,
                mouse_enabled,
                mouse.column,
                mouse.row,
            ) {
                Some(ui::HintClick::Plain('o')) | Some(ui::HintClick::Leader('o')) => {
                    // Disarm like the key path: an armed prefix left standing
                    // would consume the next key as a stale follow-up once the
                    // dialog closes.
                    ws.cancel_prefix();
                    KeyOutcome::Project(ProjectRequest::OpenDialog)
                }
                _ => KeyOutcome::Continue,
            }
        }
    }
}

/// Route a captured mouse event to the pane under the pointer.
///
/// A button press focuses that pane (mirroring a jump key), and press and
/// release are forwarded — via `click_pane` — only to a program that asked
/// for mouse reports. A release pairs with the press's pane rather than the
/// pane under the pointer (see `release_pending_press`). Wheel notches
/// scroll the pane under the pointer, not the active one, through the same
/// sink logic as the scroll keys. A left press outside pane content can
/// focus an upper panel, jump to a pane via its tab (or a `+N` hidden
/// marker), or run a hint-bar shortcut — the latter dispatched as
/// synthesized keypresses so a click and the named key take the same code
/// path (hence the `KeyOutcome` return, e.g. for `r: redraw`). While
/// pane-swap mode is armed, a left click names the swap target instead,
/// mirroring the digit follow-up. Presses on anything else (borders,
/// header) are dropped, and drag/motion reports are not forwarded at all:
/// inner-program text selection stays with the outer terminal's
/// Shift+drag.
fn handle_mouse(
    app: &mut App,
    tabs: ui::Chrome<'_>,
    mouse: MouseEvent,
    screen: Rect,
    layout: &config::LayoutConfig,
) -> KeyOutcome {
    // Releases route by the pending press, not the pointer, so they must be
    // handled before the hit test — the pointer may have left the pane (or
    // every pane) between press and release. They also bypass the modal
    // guard below: the press happened before the modal opened, and the
    // program that saw it must still see the release — swallowing it would
    // leave the pending slot stale for a later unrelated release.
    if let MouseEventKind::Up(_) = mouse.kind {
        release_pending_press(app, screen, layout, mouse.column, mouse.row);
        return KeyOutcome::Continue;
    }
    // Modal overlays (repo-switch dialog, every search bar) own all other
    // input while open — same rule the key handler enforces: a click behind
    // a modal must not move focus or reach a pane.
    if app.search_overlay_active() {
        return KeyOutcome::Continue;
    }
    // Pane-swap mode: a press names the swap target the way a digit does —
    // a left click on a pane or its tab swaps the active pane with it, and
    // any other press consumes-and-disarms, mirroring the key follow-up
    // (`handle_swap_target_followup`). Without this branch a click would
    // change the active pane while leaving swap mode armed, so a later
    // digit would swap the wrong pane. Wheel events fall through, like a
    // paste: they don't name a pane and don't disturb the armed state.
    if app.awaiting_swap_target()
        && let MouseEventKind::Down(button) = mouse.kind
    {
        app.cancel_swap_target();
        if button == crossterm::event::MouseButton::Left {
            let target = ui::pane_at(app, screen, layout, mouse.column, mouse.row)
                .and_then(|(id, _)| app.terminal.panes.iter().position(|p| p.id == id))
                .or_else(|| ui::tab_click_at(app, screen, layout, mouse.column, mouse.row));
            if let Some(idx) = target {
                app.swap_active_pane_with(idx);
            }
        }
        return KeyOutcome::Continue;
    }
    let Some((id, rect)) = ui::pane_at(app, screen, layout, mouse.column, mouse.row) else {
        // Not a terminal cell: a press can still focus an upper panel
        // (file/commit/tree list or diff viewer) in the normal split layout,
        // or run a shortcut named on the bottom hint row.
        if let MouseEventKind::Down(button) = mouse.kind {
            // The project tab row is checked first: it sits above the body, so
            // no panel hit test can claim it, and a tab click is the pointer
            // equivalent of its F-key.
            if button == crossterm::event::MouseButton::Left
                && let Some(idx) = ui::project_tab_at(tabs, screen, mouse.column, mouse.row)
            {
                app.cancel_prefix();
                return KeyOutcome::Project(ProjectRequest::Switch(idx));
            }
            if let Some(focus) = ui::upper_panel_at(app, screen, layout, mouse.column, mouse.row) {
                app.cancel_prefix();
                app.focus = focus;
            } else if button == crossterm::event::MouseButton::Left {
                if let Some(idx) = ui::tab_click_at(app, screen, layout, mouse.column, mouse.row) {
                    // A tab click is a jump-key press with the pointer: same
                    // prefix resolution and focus/fullscreen handling.
                    app.cancel_prefix();
                    app.switch_pane(idx);
                } else if let Some(click) =
                    ui::hint_click_at(app, tabs, screen, mouse.column, mouse.row)
                {
                    return dispatch_hint_click(app, click);
                }
            }
        }
        return KeyOutcome::Continue;
    };
    // 1-based pane-local cell, as SGR reports expect. In-bounds by
    // construction: `pane_at` only returns a rect containing the cell.
    let col = mouse.column - rect.x + 1;
    let row = mouse.row - rect.y + 1;
    match mouse.kind {
        MouseEventKind::Down(button) => {
            focus_clicked_pane(app, id);
            if app.terminal.click_pane(id, button, true, col, row) {
                app.pending_mouse_press = Some((id, button, col, row));
            }
        }
        MouseEventKind::ScrollUp => {
            app.terminal
                .scroll_pane(id, true, WHEEL_LINES_PER_NOTCH, Some((col, row)));
        }
        MouseEventKind::ScrollDown => {
            app.terminal
                .scroll_pane(id, false, WHEEL_LINES_PER_NOTCH, Some((col, row)));
        }
        // Horizontal wheel has no scrollback fallback; it reaches only a
        // pane whose program asked for wheel reports (trackpads and tilt
        // wheels in e.g. a full-screen TUI with horizontal panes).
        MouseEventKind::ScrollLeft => {
            app.terminal.wheel_horizontal_pane(id, true, col, row);
        }
        MouseEventKind::ScrollRight => {
            app.terminal.wheel_horizontal_pane(id, false, col, row);
        }
        _ => {}
    }
    KeyOutcome::Continue
}

/// Run a clicked hint-bar shortcut by synthesizing the keypress(es) its
/// label names, so a click and the real key share every guard and dispatch
/// path in `handle_key` — a hint click can never do something the named key
/// would not. `Arm` hints press the leader chord alone (the armed row then
/// offers clickable follow-ups); `Leader` hints press the leader chord first
/// (arming the prefix) and the follow-up second; `Plain` hints press one
/// bare key.
fn dispatch_hint_click(app: &mut App, click: ui::HintClick) -> KeyOutcome {
    let plain = |c| KeyEvent::new(KeyCode::Char(c), KeyModifiers::NONE);
    match click {
        ui::HintClick::Arm => {
            let leader = app.leader;
            handle_key(app, leader)
        }
        ui::HintClick::Leader(c) => {
            let leader = app.leader;
            match handle_key(app, leader) {
                KeyOutcome::Continue => {}
                other => return other,
            }
            handle_key(app, plain(c))
        }
        ui::HintClick::Plain(c) => handle_key(app, plain(c)),
    }
}

/// Deliver a button release to the pane that received the matching press.
///
/// A program that saw an SGR press must see the release even when the
/// pointer moved off the pane in between (no drag reports are forwarded, so
/// it cannot track the pointer itself) — and a pane the pointer merely ends
/// up over must NOT receive a release it never got a press for. The release
/// carries the *stored* press button, not the one crossterm reported:
/// legacy encodings don't identify the button on release, so some
/// platforms report every `Up` as `Left`, and trusting that would strand a
/// right/middle press without its release. Chords were never paired (the
/// slot is single), so any release closes the pending press. The release
/// cell is clamped into the pressed pane's current rect. If that pane was
/// closed or hidden since the press, the release is dropped.
fn release_pending_press(
    app: &mut App,
    screen: Rect,
    layout: &config::LayoutConfig,
    x: u16,
    y: u16,
) {
    let Some((id, pressed, _, _)) = app.pending_mouse_press else {
        return;
    };
    app.pending_mouse_press = None;
    let Some(rect) = ui::terminal_content_areas(app, screen, layout)
        .into_iter()
        .find_map(|(pid, rect)| (pid == id).then_some(rect))
    else {
        return;
    };
    // An extreme resize between press and release can shrink the pane to a
    // zero-sized rect, which would invert the clamp bounds below (`clamp`
    // panics when min > max).
    if rect.width == 0 || rect.height == 0 {
        return;
    }
    let col = x.clamp(rect.x, rect.right() - 1) - rect.x + 1;
    let row = y.clamp(rect.y, rect.bottom() - 1) - rect.y + 1;
    app.terminal.click_pane(id, pressed, false, col, row);
}

/// Make the clicked pane active and move focus to the terminal, exactly what
/// a jump key does. A click is also a non-command event while the prefix is
/// armed, so resolve the prefix first (same rule as `handle_paste`).
fn focus_clicked_pane(app: &mut App, id: backend::PaneId) {
    app.cancel_prefix();
    let Some(idx) = app.terminal.panes.iter().position(|p| p.id == id) else {
        return;
    };
    app.terminal.active = idx;
    app.terminal.sync_visible_window();
    app.focus = Focus::Terminal;
}

/// Route pasted text: into the open repo dialog if it owns input, else to the
/// active project. Nothing happens with no project and no dialog — there is no
/// sink for it.
fn dispatch_paste(ws: &mut Workspace, text: &str) {
    if ws.repo_input.active {
        for ch in text.chars().filter(|c| !c.is_control()) {
            ws.repo_input_push(ch);
        }
        return;
    }
    match ws.active_mut() {
        Some(app) => handle_paste(app, text),
        // No sink for the text, but an armed prefix must still resolve — a
        // non-command event cancels it, as it does on the project screen.
        None => ws.cancel_prefix(),
    }
}

/// Route a bracketed-paste payload within one project.
///
/// Its search overlays accept the text after stripping control characters —
/// the same rule the typed-key handlers enforce. The terminal pane receives
/// the paste re-wrapped in `ESC [200~ ... ESC [201~` so the inner shell can
/// distinguish multi-line paste from interactive input (crossterm consumes the
/// outer markers when surfacing `Event::Paste`).
fn handle_paste(app: &mut App, text: &str) {
    // A paste arriving while the prefix is armed would otherwise leave the
    // PREFIX indicator stuck and make the next key resolve as a follow-up.
    // Resolve the prefix first (tmux treats a non-command event as a cancel),
    // then route the paste normally.
    app.cancel_prefix();
    if app.focus == Focus::FileList && app.status_view.search_active {
        for ch in text.chars().filter(|c| !c.is_control()) {
            app.search_push(ch);
        }
        return;
    }
    if app.focus == Focus::FileList && app.tree_view.search_active {
        for ch in text.chars().filter(|c| !c.is_control()) {
            app.tree_search_push(ch);
        }
        return;
    }
    if app.focus == Focus::FileList
        && (app.log_view.commit_search_active || app.log_view.file_search_active)
    {
        for ch in text.chars().filter(|c| !c.is_control()) {
            app.log_search_push(ch);
        }
        return;
    }
    if app.focus == Focus::DiffViewer && app.diff.search.active {
        for ch in text.chars().filter(|c| !c.is_control()) {
            app.diff.search_push(ch);
        }
        return;
    }
    if app.focus == Focus::Terminal {
        // Strip ESC (0x1b) and NUL (0x00) before forwarding: an embedded
        // 0x1b can re-arm or cancel the bracketed-paste boundary the shell
        // is parsing, and NUL is malformed for most line-buffered shells.
        // Newlines, tabs, and other printable controls stay in — they are
        // exactly what bracketed paste is meant to deliver atomically.
        let sanitized: Vec<u8> = text
            .as_bytes()
            .iter()
            .copied()
            .filter(|&b| b != 0x1b && b != 0x00)
            .collect();
        // Only wrap in bracketed-paste markers when the running program asked
        // for them (DECSET 2004). A raw program that never enabled the mode
        // would otherwise receive the literal `[200~`/`[201~` markers as input.
        let bracketed = app
            .active_screen()
            .map(|screen| screen.bracketed_paste())
            .unwrap_or(false);
        if bracketed {
            let mut bytes = Vec::with_capacity(sanitized.len() + 12);
            bytes.extend_from_slice(b"\x1b[200~");
            bytes.extend_from_slice(&sanitized);
            bytes.extend_from_slice(b"\x1b[201~");
            app.terminal.send_input(&bytes);
        } else {
            app.terminal.send_input(&sanitized);
        }
    }
}

fn handle_key(app: &mut App, key: KeyEvent) -> KeyOutcome {
    // Crossterm emits Press/Repeat/Release for every keystroke on Windows
    // and on terminals that negotiate the kitty keyboard protocol.
    // Without this guard every keypress would be processed twice or more
    // — visible as doubled search chars, the leader firing repeatedly, and
    // Backspace popping past the buffer.
    if key.kind != KeyEventKind::Press {
        return KeyOutcome::Continue;
    }

    // A key nightcrow acts on itself means the user has moved on, so the
    // notice row goes back to showing repo identity. Keys forwarded verbatim
    // to a PTY are excluded: in a terminal pane every keystroke is
    // passthrough, and dismissing on those would blank a notice the moment
    // the user resumed typing. Runs before dispatch so an action that raises
    // a *new* notice still leaves it standing.
    if app.search_overlay_active()
        || app.prefix_armed()
        || app.awaiting_swap_target()
        || app.is_leader_key(key)
        || app.focus != Focus::Terminal
    {
        app.dismiss_notice_on_app_input();
    }

    // Modal overlays (repo-input dialog, both search bars) own every
    // keystroke until dismissed. They are checked before any leader handling
    // so a leader keypress while a search/repo dialog is open is typed/edited
    // by the overlay rather than arming the prefix.
    if app.search_overlay_active() {
        // A prefix (or swap-target) could only be armed if an overlay opened
        // out from under it; disarm both so neither indicator lingers behind a
        // modal.
        app.cancel_prefix();
        app.cancel_swap_target();
        // Search overlays are handled inside the focus-local upper handler.
        handle_upper_key(app, key, Action::None);
        return KeyOutcome::Continue;
    }

    // Swap-target mode is armed (`<leader> s`): this key is the digit naming
    // the pane to swap the active pane with. Checked before the prefix so its
    // dedicated follow-up handler owns the key.
    if app.awaiting_swap_target() {
        return handle_swap_target_followup(app, key);
    }

    // Prefix is armed: this key is the single follow-up. Resolve it three
    // ways — Esc/Ctrl+C cancels, the leader again sends a literal leader to
    // the PTY, a mapped key runs its action; any other key is consumed.
    if app.prefix_armed() {
        return handle_prefix_followup(app, key);
    }

    // The leader chord arms the prefix; nothing else happens this tick.
    if app.is_leader_key(key) {
        app.arm_prefix();
        return KeyOutcome::Continue;
    }

    let action = map_key(key);
    if let Some(outcome) = handle_global_action(app, action) {
        return outcome;
    }

    match app.focus {
        Focus::Terminal => handle_terminal_key(app, key, action),
        Focus::FileList | Focus::DiffViewer => handle_upper_key(app, key, action),
    }
    KeyOutcome::Continue
}

/// Resolve the single key pressed while the prefix is armed. The prefix is
/// always disarmed before returning (tmux-style: one follow-up per leader).
fn handle_prefix_followup(app: &mut App, key: KeyEvent) -> KeyOutcome {
    app.cancel_prefix();

    // `<L> <L>`: send the leader chord literally to the focused PTY so the
    // running program still sees the prefix key when the user means it. This
    // is resolved before the Esc/Ctrl+C cancel below so that a `ctrl+c` leader
    // can still deliver a literal Ctrl+C via `<leader><leader>` (Esc remains a
    // universal cancel regardless of the configured leader).
    if app.is_leader_key(key) {
        if app.focus == Focus::Terminal
            && let Some(data) = encode_key(app.leader)
        {
            app.terminal.send_input(&data);
        }
        return KeyOutcome::Continue;
    }

    // Esc / Ctrl+C cancel the prefix without acting. The follow-up key is
    // consumed (not forwarded) so the cancel never leaks into the PTY.
    let is_ctrl_c = key.code == KeyCode::Char('c') && key.modifiers.contains(KeyModifiers::CONTROL);
    if key.code == KeyCode::Esc || is_ctrl_c {
        return KeyOutcome::Continue;
    }

    // A mapped follow-up runs its app command everywhere (terminal + upper).
    let action = resolve_prefix_action(app, key);
    if let Some(outcome) = handle_global_action(app, action) {
        return outcome;
    }
    // Unmapped follow-up: consume and drop it, then return to pass-through.
    KeyOutcome::Continue
}

/// Resolve the key pressed while swap-target mode is armed (`<leader> s`). The
/// mode is always disarmed before returning. A digit that names a pane runs the
/// swap; `Esc`/`Ctrl+C` cancels; any other key is consumed. The digit→pane
/// mapping is reused from `prefix_action` so it matches the focus-jump digits
/// one-for-one (`3`..`9`,`0` → panes `0`..`7`).
fn handle_swap_target_followup(app: &mut App, key: KeyEvent) -> KeyOutcome {
    app.cancel_swap_target();

    let is_ctrl_c = key.code == KeyCode::Char('c') && key.modifiers.contains(KeyModifiers::CONTROL);
    if key.code == KeyCode::Esc || is_ctrl_c {
        return KeyOutcome::Continue;
    }

    if let Action::SwitchPane(idx) = resolve_prefix_action(app, key) {
        app.swap_active_pane_with(idx);
    }
    KeyOutcome::Continue
}

/// Pick the leader follow-up mapping for the current layout. While the terminal
/// fills the body the upper viewer is hidden, so `prefix_action_fullscreen`
/// repurposes the digit row `1`..`8` onto panes `0`..`7`; otherwise the normal
/// split-view mapping applies (`1`=list, `2`=diff, `3`..`0`=panes). Shared by
/// the focus-jump and swap-target follow-ups so both stay in lockstep.
fn resolve_prefix_action(app: &App, key: KeyEvent) -> Action {
    if app.terminal.fullscreen.fills_body() {
        prefix_action_fullscreen(key)
    } else {
        prefix_action(key)
    }
}

fn handle_global_action(app: &mut App, action: Action) -> Option<KeyOutcome> {
    match action {
        Action::Quit => Some(KeyOutcome::Quit),
        Action::NewPane => {
            app.open_new_pane();
            Some(KeyOutcome::Continue)
        }
        Action::ClosePane => {
            // Scoped by `can_close_pane` (terminal focus — the close target
            // is invisible without it). The key is still consumed so it
            // can't leak elsewhere.
            if app.can_close_pane() {
                app.close_active_pane();
            }
            Some(KeyOutcome::Continue)
        }
        // Opening is two steps: this only raises the dialog, and confirming it
        // emits the `Open` request (see `handle_repo_input_key`).
        Action::OpenProject => Some(KeyOutcome::Project(ProjectRequest::OpenDialog)),
        Action::CloseProject => Some(KeyOutcome::Project(ProjectRequest::Close)),
        Action::SwitchProject(idx) => Some(KeyOutcome::Project(ProjectRequest::Switch(idx))),
        Action::ToggleFullscreen => {
            match app.focus {
                Focus::DiffViewer => app.toggle_diff_fullscreen(),
                Focus::FileList => app.toggle_list_fullscreen(),
                Focus::Terminal => app.toggle_terminal_fullscreen(),
            }
            Some(KeyOutcome::Continue)
        }
        Action::ToggleLogView => {
            app.toggle_mode();
            Some(KeyOutcome::Continue)
        }
        Action::ToggleTreeView => {
            app.toggle_tree_mode();
            Some(KeyOutcome::Continue)
        }
        Action::CycleTheme => {
            app.cycle_accent();
            Some(KeyOutcome::Continue)
        }
        Action::Redraw => Some(KeyOutcome::Redraw),
        Action::SwitchPane(n) => {
            app.switch_pane(n);
            Some(KeyOutcome::Continue)
        }
        Action::SwapPanePrompt => {
            // Scoped by `can_swap_panes` (terminal focus plus a second pane).
            // The key is still consumed either way.
            if app.can_swap_panes() {
                app.begin_swap_target();
            }
            Some(KeyOutcome::Continue)
        }
        Action::FocusList => {
            app.focus_list();
            Some(KeyOutcome::Continue)
        }
        Action::FocusDiff => {
            app.focus_diff();
            Some(KeyOutcome::Continue)
        }
        Action::CycleForward => {
            app.cycle_focus_forward();
            Some(KeyOutcome::Continue)
        }
        Action::CycleBackward => {
            app.cycle_focus_backward();
            Some(KeyOutcome::Continue)
        }
        _ => None,
    }
}

fn has_command_modifier(key: KeyEvent) -> bool {
    key.modifiers.intersects(
        KeyModifiers::CONTROL
            | KeyModifiers::ALT
            | KeyModifiers::SUPER
            | KeyModifiers::HYPER
            | KeyModifiers::META,
    )
}

fn text_input_char(key: KeyEvent) -> Option<char> {
    if has_command_modifier(key) {
        return None;
    }
    match key.code {
        KeyCode::Char(c) if !c.is_control() => Some(c),
        _ => None,
    }
}

fn matches_text_command(key: KeyEvent, expected: char) -> bool {
    !has_command_modifier(key) && matches!(key.code, KeyCode::Char(c) if c == expected)
}

/// Route one key, resolving the workspace-level cases first.
///
/// The open dialog and the empty screen both belong to the workspace, and
/// `handle_key` holds a single project, so neither can be dispatched from
/// inside it. Resolving them here keeps `handle_key` — and every test that
/// drives it with one `App` — working on exactly one project.
fn dispatch_key(ws: &mut Workspace, key: KeyEvent) -> KeyOutcome {
    if key.kind != KeyEventKind::Press {
        return KeyOutcome::Continue;
    }
    if ws.repo_input.active {
        return handle_repo_input_key(ws, key);
    }
    match ws.active_mut() {
        Some(app) => handle_key(app, key),
        None => handle_empty_key(ws, key),
    }
}

/// Keys on the empty screen: the leader arms, `o` opens the dialog, `q`
/// quits. Everything else is dropped — there is no project to act on and no
/// PTY to forward to.
fn handle_empty_key(ws: &mut Workspace, key: KeyEvent) -> KeyOutcome {
    if ws.prefix_armed() {
        ws.cancel_prefix();
        // `<L> <L>` sends a literal leader to the focused PTY on the project
        // screen; here there is no pane to send it to, so it is consumed.
        // Resolving it before the action table matters: with the default
        // `ctrl+f` leader the follow-up would otherwise match `f` and toggle
        // fullscreen.
        if ws.is_leader_key(key) {
            return KeyOutcome::Continue;
        }
        return match prefix_action(key) {
            Action::OpenProject => KeyOutcome::Project(ProjectRequest::OpenDialog),
            Action::Quit => KeyOutcome::Quit,
            _ => KeyOutcome::Continue,
        };
    }
    if ws.is_leader_key(key) {
        ws.arm_prefix();
    }
    KeyOutcome::Continue
}

fn handle_repo_input_key(ws: &mut Workspace, key: KeyEvent) -> KeyOutcome {
    match key.code {
        KeyCode::Esc => ws.cancel_repo_input(),
        KeyCode::Enter => {
            if let workspace::RepoInputResult::Open(path) = ws.confirm_repo_input() {
                return KeyOutcome::Project(ProjectRequest::Open(path));
            }
        }
        KeyCode::Backspace => {
            if ws.repo_input.buf.is_empty() {
                ws.cancel_repo_input();
            } else {
                ws.repo_input_pop();
            }
        }
        // The caret is always at the end of the buffer, so these can't move
        // it; they mean "keep this path and let me extend it".
        KeyCode::Right | KeyCode::End => ws.repo_input_accept_prefill(),
        _ => {
            if let Some(c) = text_input_char(key) {
                ws.repo_input_push(c);
            }
        }
    }
    KeyOutcome::Continue
}

fn handle_terminal_key(app: &mut App, key: KeyEvent, action: Action) {
    match action {
        Action::TermScrollUp => {
            let lines = app.terminal.active_pane_rows();
            app.terminal.scroll_active(true, lines);
        }
        Action::TermScrollDown => {
            let lines = app.terminal.active_pane_rows();
            app.terminal.scroll_active(false, lines);
        }
        Action::TermScrollLineUp => app.terminal.scroll_active(true, SCROLL_LINE_STEP),
        Action::TermScrollLineDown => app.terminal.scroll_active(false, SCROLL_LINE_STEP),
        _ => {
            if let Some(data) = encode_key(key) {
                app.terminal.send_input(&data);
            }
        }
    }
}

fn handle_upper_key(app: &mut App, key: KeyEvent, action: Action) {
    if app.focus == Focus::FileList && app.status_view.search_active {
        handle_file_search_key(app, key);
        return;
    }
    if app.focus == Focus::FileList && app.tree_view.search_active {
        handle_tree_search_key(app, key);
        return;
    }
    if app.focus == Focus::FileList
        && (app.log_view.commit_search_active || app.log_view.file_search_active)
    {
        handle_log_search_key(app, key);
        return;
    }
    if app.focus == Focus::DiffViewer && app.diff.search.active {
        handle_diff_search_key(app, key);
        return;
    }

    // Apply vim-style j/k navigation only in upper panes; terminal focus is
    // routed through handle_terminal_key so j/k reach the PTY untouched.
    let action = vim_navigation_action(key).unwrap_or(action);

    match action {
        Action::Up => app.select_up(),
        Action::Down => app.select_down(),
        Action::PageUp => app.page_up(),
        Action::PageDown => app.page_down(),
        Action::TermScrollUp
        | Action::TermScrollDown
        | Action::TermScrollLineUp
        | Action::TermScrollLineDown => {}
        Action::None => handle_unmapped_upper_key(app, key),
        _ => {}
    }
}

fn handle_file_search_key(app: &mut App, key: KeyEvent) {
    match key.code {
        KeyCode::Up => app.select_up(),
        KeyCode::Down => app.select_down(),
        KeyCode::Esc => app.cancel_search(),
        KeyCode::Enter => app.confirm_search(),
        KeyCode::Backspace => {
            if app.status_view.search_query.is_empty() {
                app.cancel_search();
            } else {
                app.search_pop();
            }
        }
        _ => {
            // Reject command chords: Ctrl+letter reaches crossterm as the
            // literal letter, not as a control char, so modifier state is the
            // reliable guard against polluting the query.
            if let Some(c) = text_input_char(key) {
                app.search_push(c);
            }
        }
    }
}

fn handle_tree_search_key(app: &mut App, key: KeyEvent) {
    match key.code {
        KeyCode::Up => app.select_up(),
        KeyCode::Down => app.select_down(),
        KeyCode::Esc => app.cancel_tree_search(),
        KeyCode::Enter => app.confirm_tree_search(),
        KeyCode::Backspace => {
            if app.tree_view.search_query.is_empty() {
                app.cancel_tree_search();
            } else {
                app.tree_search_pop();
            }
        }
        _ => {
            // Same chord guard as the file search: Ctrl+letter arrives as the
            // bare letter, so modifier state is what keeps it out of the query.
            if let Some(c) = text_input_char(key) {
                app.tree_search_push(c);
            }
        }
    }
}

fn handle_log_search_key(app: &mut App, key: KeyEvent) {
    match key.code {
        KeyCode::Up => app.select_up(),
        KeyCode::Down => app.select_down(),
        KeyCode::Esc => app.cancel_log_search(),
        KeyCode::Enter => app.confirm_log_search(),
        KeyCode::Backspace => {
            // Which query is active depends on whether the drill-down file
            // list is showing; mirror the dispatch used by `log_search_push`.
            let query_empty = if app.log_view.drill_down {
                app.log_view.file_search_query.is_empty()
            } else {
                app.log_view.commit_search_query.is_empty()
            };
            if query_empty {
                app.cancel_log_search();
            } else {
                app.log_search_pop();
            }
        }
        _ => {
            if let Some(c) = text_input_char(key) {
                app.log_search_push(c);
            }
        }
    }
}

fn handle_diff_search_key(app: &mut App, key: KeyEvent) {
    match key.code {
        KeyCode::Esc => app.diff.cancel_search(),
        KeyCode::Enter => app.diff.confirm_search(),
        KeyCode::Backspace => {
            if app.diff.search.query.is_empty() {
                app.diff.cancel_search();
            } else {
                app.diff.search_pop();
            }
        }
        _ => {
            if let Some(c) = text_input_char(key) {
                app.diff.search_push(c);
            }
        }
    }
}

fn handle_unmapped_upper_key(app: &mut App, key: KeyEvent) {
    match app.focus {
        Focus::FileList => match key.code {
            KeyCode::Enter if app.mode == ViewMode::Log && !app.log_view.drill_down => {
                app.log_drill_in()
            }
            // Tree navigation: Enter toggles a directory (or re-previews a
            // file), Right expands, Left collapses / steps to the parent. These
            // guarded arms shadow the generic Left/Right horizontal-scroll arms
            // below while in Tree mode.
            KeyCode::Enter if app.mode == ViewMode::Tree => app.tree_toggle(),
            KeyCode::Right if app.mode == ViewMode::Tree => app.tree_expand(),
            KeyCode::Left if app.mode == ViewMode::Tree => app.tree_collapse(),
            // Log search Esc precedence sits ahead of `log_drill_out` so the
            // first Esc clears a confirmed filter before a second Esc exits
            // drill-down — mirrors the status-search Esc rule below.
            KeyCode::Esc
                if app.mode == ViewMode::Log
                    && app.log_view.drill_down
                    && !app.log_view.file_search_query.is_empty() =>
            {
                app.cancel_log_search()
            }
            KeyCode::Esc
                if app.mode == ViewMode::Log
                    && !app.log_view.drill_down
                    && !app.log_view.commit_search_query.is_empty() =>
            {
                app.cancel_log_search()
            }
            KeyCode::Esc if app.log_view.drill_down => app.log_drill_out(),
            _ if app.mode == ViewMode::Status && matches_text_command(key, '/') => {
                app.start_search()
            }
            _ if app.mode == ViewMode::Tree && matches_text_command(key, '/') => {
                app.start_tree_search()
            }
            _ if app.mode == ViewMode::Log && matches_text_command(key, '/') => {
                app.start_log_search()
            }
            KeyCode::Esc if !app.status_view.search_query.is_empty() => app.cancel_search(),
            KeyCode::Left => app.file_scroll_left(),
            KeyCode::Right => app.file_scroll_right(),
            _ => {}
        },
        Focus::DiffViewer => match key.code {
            _ if matches_text_command(key, 'v') => app.toggle_diff_file_view(),
            _ if matches_text_command(key, 's') => app.toggle_diff_split_view(),
            _ if matches_text_command(key, '/') => {
                exit_split_for_search(app);
                app.diff.start_search();
            }
            _ if matches_text_command(key, 'n') && app.diff.search.has_query() => {
                exit_split_for_search(app);
                app.diff.next_match();
            }
            _ if matches_text_command(key, 'N') && app.diff.search.has_query() => {
                exit_split_for_search(app);
                app.diff.prev_match();
            }
            KeyCode::Esc if !app.diff.search.query.is_empty() => app.diff.cancel_search(),
            KeyCode::Left => app.diff.scroll_left(),
            KeyCode::Right => app.diff.scroll_right(),
            _ => {}
        },
        Focus::Terminal => {}
    }
}

fn exit_split_for_search(app: &mut App) {
    if app.diff.view == DiffPaneView::Split {
        app.diff.view = DiffPaneView::Diff;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::DiffPaneView;
    use crate::app::tests::{app_with_fake_backend, app_with_files};
    use crossterm::event::KeyModifiers;

    fn press(code: KeyCode, mods: KeyModifiers) -> KeyEvent {
        KeyEvent::new(code, mods)
    }

    /// The default leader chord (Ctrl+F). Test apps all use the default, so a
    /// standalone constructor avoids borrowing `app` inside a `handle_key`
    /// call (which would conflict with the `&mut app` argument).
    fn leader() -> KeyEvent {
        KeyEvent::new(KeyCode::Char('f'), KeyModifiers::CONTROL)
    }

    /// Snapshot the byte payloads the app's `FakeBackend` recorded so terminal
    /// pass-through and literal-leader tests can assert exact PTY bytes.
    fn backend_payloads(app: &App) -> Vec<Vec<u8>> {
        app.terminal
            .fake_backend_sent()
            .expect("test app must use a FakeBackend")
    }

    /// A FakeBackend-backed app with one open terminal pane and terminal
    /// focus, ready for PTY pass-through assertions.
    fn app_with_terminal_pane() -> App {
        let mut app = app_with_fake_backend();
        app.terminal.create_pane().unwrap();
        app.focus = Focus::Terminal;
        app
    }

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

    /// A workspace of projects distinguished by `repo_path`, plus the context
    /// `apply_project_request` needs. `Open` is the only request that builds a
    /// project, so a default config suffices for the rest.
    fn workspace_on(paths: &[&str]) -> Workspace {
        let project = |p: &str| {
            let mut app = app_with_files(vec!["a.rs"]);
            app.repo_path = p.to_string();
            app
        };
        let mut ws = Workspace::new(leader());
        for p in paths {
            assert!(ws.add(project(p)));
        }
        ws
    }

    #[test]
    fn opening_a_repo_another_tab_holds_focuses_it_instead_of_duplicating() {
        let cfg = config::Config::default();
        let ctx = ProjectContext {
            cfg: &cfg,
            startup_commands: &[],
            leader: leader(),
        };
        let mut ws = workspace_on(&["/a", "/b"]);

        apply_project_request(&mut ws, &ctx, ProjectRequest::Open("/a".to_string()));

        assert_eq!(ws.active_index(), 0);
        assert_eq!(ws.projects().len(), 2);
    }

    #[test]
    fn clicking_a_project_tab_asks_the_workspace_to_switch() {
        let mut app = app_with_fake_backend();
        let tabs = vec!["/w/api".to_string(), "/w/web".to_string()];
        // Column 0 of row 0 is the first tab; a click there is the pointer
        // equivalent of pressing F1.
        let outcome = handle_mouse(
            &mut app,
            ui::Chrome {
                repo_paths: &tabs,
                active: 1,
                repo_input: &ui::status_view::RepoInput::default(),
            },
            mouse(
                MouseEventKind::Down(crossterm::event::MouseButton::Left),
                0,
                0,
            ),
            MOUSE_TEST_SCREEN,
            &config::LayoutConfig::default(),
        );

        assert_eq!(outcome, KeyOutcome::Project(ProjectRequest::Switch(0)));
    }

    #[test]
    fn f_key_asks_the_workspace_to_switch_project() {
        let mut app = app_with_files(vec!["a.rs"]);

        // Bare F-keys need no prefix, and the request is emitted rather than
        // acted on: the handler holds one project and cannot reach the tabs.
        let outcome = handle_key(&mut app, press(KeyCode::F(3), KeyModifiers::NONE));

        assert_eq!(outcome, KeyOutcome::Project(ProjectRequest::Switch(2)));
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
    fn confirming_the_dialog_asks_the_workspace_to_open_that_path() {
        let (_dir, path) = crate::test_util::make_repo();
        let mut ws = workspace_on(&["/a"]);
        ws.start_repo_input();
        for c in path.chars() {
            ws.repo_input_push(c);
        }

        let outcome = dispatch_key(&mut ws, press(KeyCode::Enter, KeyModifiers::NONE));

        // The emitted path is the *resolved* workdir, not the typed text —
        // on macOS the temp dir reaches it through a /var -> /private/var
        // symlink, and the workdir carries a trailing separator.
        let expected = git::resolve_repo_path(std::path::Path::new(&path))
            .to_string_lossy()
            .to_string();
        assert_eq!(outcome, KeyOutcome::Project(ProjectRequest::Open(expected)));
        // The current project still points at its original repo: confirming
        // opens a tab, it never repoints this one.
        assert_eq!(ws.active().unwrap().repo_path, "/a");
        assert!(!ws.repo_input.active, "dialog must close on success");
    }

    #[test]
    fn confirming_the_dialog_on_a_bad_path_keeps_it_open() {
        let mut ws = workspace_on(&["/a"]);
        ws.start_repo_input();
        for c in "/definitely/not/a/directory".chars() {
            ws.repo_input_push(c);
        }

        let outcome = dispatch_key(&mut ws, press(KeyCode::Enter, KeyModifiers::NONE));

        assert_eq!(outcome, KeyOutcome::Continue);
        assert!(ws.repo_input.active, "a rejected path must stay editable");
    }

    #[test]
    fn the_empty_screen_opens_the_dialog_and_quits() {
        let mut ws = Workspace::new(leader());
        assert!(ws.active().is_none());

        // The leader still arms with no project, and only `o` and `q` resolve.
        let _ = dispatch_key(&mut ws, leader());
        let open = dispatch_key(&mut ws, press(KeyCode::Char('o'), KeyModifiers::NONE));
        assert_eq!(open, KeyOutcome::Project(ProjectRequest::OpenDialog));

        let _ = dispatch_key(&mut ws, leader());
        let quit = dispatch_key(&mut ws, press(KeyCode::Char('q'), KeyModifiers::NONE));
        assert_eq!(quit, KeyOutcome::Quit);

        // An unbound follow-up is consumed, not forwarded anywhere.
        let _ = dispatch_key(&mut ws, leader());
        let other = dispatch_key(&mut ws, press(KeyCode::Char('t'), KeyModifiers::NONE));
        assert_eq!(other, KeyOutcome::Continue);
    }

    #[test]
    fn the_dialog_still_lets_a_pending_release_through() {
        // A modal opening between press and release must not strand the
        // pending slot: the pane that saw the press has to see the release, and
        // a leftover slot would pair with a later unrelated one.
        let (mut app, areas) = app_with_two_panes_and_areas();
        let (id, rect) = areas[0];
        // Only a pane whose program asked for mouse reports records a pending
        // press, so opt it in.
        app.terminal
            .emulators
            .get_mut(&id)
            .unwrap()
            .process(b"\x1b[?1000h\x1b[?1006h");
        let mut ws = Workspace::new(leader());
        ws.add(app);
        let down = MouseEventKind::Down(crossterm::event::MouseButton::Left);
        let up = MouseEventKind::Up(crossterm::event::MouseButton::Left);
        let tabs = test_tabs();

        dispatch_mouse(
            &mut ws,
            test_tab_view(&tabs),
            mouse(down, rect.x, rect.y),
            MOUSE_TEST_SCREEN,
            &config::LayoutConfig::default(),
            true,
        );
        assert!(ws.active().unwrap().pending_mouse_press.is_some());

        ws.start_repo_input();
        dispatch_mouse(
            &mut ws,
            test_tab_view(&tabs),
            mouse(up, rect.x, rect.y),
            MOUSE_TEST_SCREEN,
            &config::LayoutConfig::default(),
            true,
        );

        assert!(
            ws.active().unwrap().pending_mouse_press.is_none(),
            "the dialog must not swallow the release"
        );
    }

    #[test]
    fn switching_projects_releases_a_pending_press_to_its_own_pane() {
        // The old PTY is still alive; without a release it sits in a drag or
        // selection state forever, since drag reports are never forwarded.
        let (mut app, areas) = app_with_two_panes_and_areas();
        let (id, rect) = areas[0];
        app.terminal
            .emulators
            .get_mut(&id)
            .unwrap()
            .process(b"\x1b[?1000h\x1b[?1006h");
        let mut ws = Workspace::new(leader());
        ws.add(app);
        let tabs = test_tabs();
        dispatch_mouse(
            &mut ws,
            test_tab_view(&tabs),
            mouse(
                MouseEventKind::Down(crossterm::event::MouseButton::Left),
                rect.x,
                rect.y,
            ),
            MOUSE_TEST_SCREEN,
            &config::LayoutConfig::default(),
            true,
        );
        assert!(ws.active().unwrap().pending_mouse_press.is_some());

        ws.add(app_with_files(vec!["b.rs"]));

        let old = &ws.projects()[0];
        assert!(old.pending_mouse_press.is_none());
        assert_eq!(
            backend_payloads(old),
            vec![b"\x1b[<0;1;1M".to_vec(), b"\x1b[<0;1;1m".to_vec()],
            "the pane must see its button-up, not just lose the record"
        );
    }

    #[test]
    fn clicking_the_empty_screen_open_hint_raises_the_dialog() {
        // It is the only action that screen offers, so it must work by pointer
        // as well as by key — and it renders inverted, advertising as much.
        let mut ws = Workspace::new(leader());
        let tabs: Vec<String> = Vec::new();
        let label = app::leader_label_of(leader());
        let x = (0..MOUSE_TEST_SCREEN.width)
            .find(|&x| {
                ui::empty_hint_click_at(
                    MOUSE_TEST_SCREEN,
                    &label,
                    false,
                    true,
                    x,
                    MOUSE_TEST_SCREEN.height - 1,
                )
                .is_some()
            })
            .expect("the open hint is clickable");

        let outcome = dispatch_mouse(
            &mut ws,
            test_tab_view(&tabs),
            mouse(
                MouseEventKind::Down(crossterm::event::MouseButton::Left),
                x,
                MOUSE_TEST_SCREEN.height - 1,
            ),
            MOUSE_TEST_SCREEN,
            &config::LayoutConfig::default(),
            true,
        );

        assert_eq!(outcome, KeyOutcome::Project(ProjectRequest::OpenDialog));
    }

    #[test]
    fn clicking_the_open_hint_while_armed_disarms_the_prefix() {
        // The armed row lays out differently (chip plus bare keys), so the hit
        // test must measure that layout — and the click must disarm, or the
        // next key after the dialog closes resolves as a stale follow-up.
        let mut ws = Workspace::new(leader());
        ws.arm_prefix();
        let tabs: Vec<String> = Vec::new();
        let label = app::leader_label_of(leader());
        let row = MOUSE_TEST_SCREEN.height - 1;
        let x = (0..MOUSE_TEST_SCREEN.width)
            .find(|&x| {
                matches!(
                    ui::empty_hint_click_at(MOUSE_TEST_SCREEN, &label, true, true, x, row),
                    Some(ui::HintClick::Plain('o'))
                )
            })
            .expect("the armed open hint is clickable");

        let outcome = dispatch_mouse(
            &mut ws,
            test_tab_view(&tabs),
            mouse(
                MouseEventKind::Down(crossterm::event::MouseButton::Left),
                x,
                row,
            ),
            MOUSE_TEST_SCREEN,
            &config::LayoutConfig::default(),
            true,
        );

        assert_eq!(outcome, KeyOutcome::Project(ProjectRequest::OpenDialog));
        assert!(!ws.prefix_armed(), "the click must disarm the prefix");
    }

    #[test]
    fn the_empty_hint_is_inert_when_mouse_capture_is_disabled() {
        // The row renders plain in that case, and a browser mouse event still
        // reaches this path — a label that does not advertise itself as
        // clickable must not act like one.
        let label = app::leader_label_of(leader());
        let row = MOUSE_TEST_SCREEN.height - 1;

        assert!((0..MOUSE_TEST_SCREEN.width).all(|x| {
            ui::empty_hint_click_at(MOUSE_TEST_SCREEN, &label, false, false, x, row).is_none()
        }));
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
    fn paste_while_prefix_armed_cancels_prefix() {
        let mut app = app_with_terminal_pane();
        let _ = handle_key(&mut app, leader());
        assert!(app.prefix_armed());

        handle_paste(&mut app, "hello");

        assert!(
            !app.prefix_armed(),
            "a paste must resolve (cancel) the armed prefix"
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
    fn terminal_paste_wraps_only_when_bracketed_mode_enabled() {
        let mut app = app_with_terminal_pane();
        // The running program enables bracketed paste (DECSET 2004).
        for emulator in app.terminal.emulators.values_mut() {
            emulator.process(b"\x1b[?2004h");
        }

        handle_paste(&mut app, "hi");

        assert_eq!(
            backend_payloads(&app).concat(),
            b"\x1b[200~hi\x1b[201~".to_vec(),
            "paste must be bracketed when the program enabled DECSET 2004"
        );
    }

    #[test]
    fn terminal_paste_sends_raw_when_bracketed_mode_disabled() {
        let mut app = app_with_terminal_pane();

        handle_paste(&mut app, "hi");

        assert_eq!(
            backend_payloads(&app).concat(),
            b"hi".to_vec(),
            "without DECSET 2004 the markers must not be sent as literal input"
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

    #[test]
    fn handle_key_leader_digits_mirror_focus_and_pane_fkeys() {
        // Digits mirror the no-prefix F-keys one-for-one: 1=F1 (file list),
        // 2=F2 (diff viewer), 3..9,0=F3..F10 (terminal panes 0..7). The
        // dispatcher consumes the digit (disarming the prefix) instead of
        // forwarding it to the PTY.
        let mut app = app_with_terminal_pane();
        app.terminal
            .create_pane_with(Some("echo two"), Some("two"))
            .unwrap();
        // Pad up to 8 panes so `<prefix> 0` (pane index 7) below is a real
        // switch, not a no-op against an out-of-range index.
        for i in 2..8 {
            app.terminal
                .create_pane_with(None, Some(&format!("pane{i}")))
                .unwrap();
        }
        assert_eq!(app.terminal.panes.len(), 8);
        app.switch_pane(0);

        // <prefix> 1 → focus file list (mirrors F1)
        let _ = handle_key(&mut app, leader());
        let _ = handle_key(&mut app, press(KeyCode::Char('1'), KeyModifiers::NONE));
        assert_eq!(app.focus, Focus::FileList, "leader+1 must mirror F1");

        // <prefix> 2 → focus diff viewer (mirrors F2)
        let _ = handle_key(&mut app, leader());
        let _ = handle_key(&mut app, press(KeyCode::Char('2'), KeyModifiers::NONE));
        assert_eq!(app.focus, Focus::DiffViewer, "leader+2 must mirror F2");

        // <prefix> 4 → terminal pane 1 (mirrors F4)
        let _ = handle_key(&mut app, leader());
        let _ = handle_key(&mut app, press(KeyCode::Char('4'), KeyModifiers::NONE));
        assert_eq!(app.terminal.active, 1, "leader+4 must mirror F4 → pane 1");

        // <prefix> 0 → terminal pane 7 (mirrors F10)
        let _ = handle_key(&mut app, leader());
        let _ = handle_key(&mut app, press(KeyCode::Char('0'), KeyModifiers::NONE));
        assert_eq!(app.terminal.active, 7, "leader+0 must mirror F10 → pane 7");

        assert!(
            !app.prefix_armed(),
            "a mapped follow-up must disarm the prefix"
        );
        assert!(
            backend_payloads(&app).is_empty(),
            "a consumed leader digit must not reach the PTY"
        );
    }

    #[test]
    fn handle_key_leader_s_then_digit_swaps_active_pane() {
        // `<leader> s 5` swaps the active pane with pane index 2 (digit 5
        // mirrors F5 → pane 2) and moves focus to follow it.
        let mut app = app_with_terminal_pane();
        for i in 1..3 {
            app.terminal
                .create_pane_with(None, Some(&format!("pane{i}")))
                .unwrap();
        }
        assert_eq!(app.terminal.panes.len(), 3);
        app.switch_pane(0);
        let moving_id = app.terminal.panes[0].id;
        let target_id = app.terminal.panes[2].id;

        // `<leader> s` arms swap mode without acting.
        let _ = handle_key(&mut app, leader());
        let _ = handle_key(&mut app, press(KeyCode::Char('s'), KeyModifiers::NONE));
        assert!(app.awaiting_swap_target(), "leader+s must arm swap mode");
        assert!(!app.prefix_armed(), "swap mode must clear the prefix");

        // The digit resolves the swap.
        let _ = handle_key(&mut app, press(KeyCode::Char('5'), KeyModifiers::NONE));
        assert!(
            !app.awaiting_swap_target(),
            "the digit must disarm swap mode"
        );
        assert_eq!(app.terminal.panes[0].id, target_id);
        assert_eq!(app.terminal.panes[2].id, moving_id);
        assert_eq!(app.terminal.active, 2, "focus follows the moved pane");
        assert!(
            backend_payloads(&app).is_empty(),
            "a consumed swap digit must not reach the PTY"
        );
    }

    #[test]
    fn handle_key_leader_s_esc_cancels_without_swapping() {
        let mut app = app_with_terminal_pane();
        app.terminal.create_pane_with(None, Some("two")).unwrap();
        app.switch_pane(0);
        let order: Vec<_> = app.terminal.panes.iter().map(|p| p.id).collect();

        let _ = handle_key(&mut app, leader());
        let _ = handle_key(&mut app, press(KeyCode::Char('s'), KeyModifiers::NONE));
        let _ = handle_key(&mut app, press(KeyCode::Esc, KeyModifiers::NONE));

        assert!(!app.awaiting_swap_target());
        assert_eq!(app.terminal.active, 0);
        let after: Vec<_> = app.terminal.panes.iter().map(|p| p.id).collect();
        assert_eq!(order, after, "esc must leave pane order unchanged");
    }

    #[test]
    fn handle_key_leader_s_non_digit_cancels() {
        // A non-pane follow-up (e.g. a letter) cancels swap mode and is
        // consumed rather than swapping or reaching the PTY.
        let mut app = app_with_terminal_pane();
        app.terminal.create_pane_with(None, Some("two")).unwrap();
        app.switch_pane(0);
        let order: Vec<_> = app.terminal.panes.iter().map(|p| p.id).collect();

        let _ = handle_key(&mut app, leader());
        let _ = handle_key(&mut app, press(KeyCode::Char('s'), KeyModifiers::NONE));
        let _ = handle_key(&mut app, press(KeyCode::Char('z'), KeyModifiers::NONE));

        assert!(!app.awaiting_swap_target());
        let after: Vec<_> = app.terminal.panes.iter().map(|p| p.id).collect();
        assert_eq!(order, after);
        assert!(backend_payloads(&app).is_empty());
    }

    /// `<leader> s` shares close's terminal-focus scope: from the upper panes
    /// the active pane is rendered indistinguishable, so the chord must be
    /// consumed without arming swap mode.
    #[test]
    fn handle_key_leader_s_without_terminal_focus_does_not_arm() {
        let mut app = app_with_terminal_pane();
        app.terminal.create_pane_with(None, Some("two")).unwrap();
        app.focus = Focus::FileList;

        let _ = handle_key(&mut app, leader());
        let _ = handle_key(&mut app, press(KeyCode::Char('s'), KeyModifiers::NONE));

        assert!(
            !app.awaiting_swap_target(),
            "leader+s must not arm swap mode without terminal focus"
        );
        assert!(
            !app.prefix_armed(),
            "the follow-up must still disarm the prefix"
        );
        assert!(
            backend_payloads(&app).is_empty(),
            "the consumed chord must not reach the PTY"
        );
    }

    /// With a single pane every swap target digit would be a no-op, so the
    /// chord must not arm swap mode.
    #[test]
    fn handle_key_leader_s_with_single_pane_does_not_arm() {
        let mut app = app_with_terminal_pane();
        assert_eq!(app.terminal.panes.len(), 1);

        let _ = handle_key(&mut app, leader());
        let _ = handle_key(&mut app, press(KeyCode::Char('s'), KeyModifiers::NONE));

        assert!(
            !app.awaiting_swap_target(),
            "leader+s must not arm swap mode with a single pane"
        );
        assert!(backend_payloads(&app).is_empty());
    }

    #[test]
    fn handle_key_leader_b_toggles_tree_mode() {
        // `<prefix> b` enters Tree mode and a second `<prefix> b` returns to
        // Status. Uses the live cwd repo (the crate root) for the root read.
        let mut app = app_with_files(vec!["a.rs"]);
        app.focus = Focus::FileList;
        assert_eq!(app.mode, ViewMode::Status);

        let _ = handle_key(&mut app, leader());
        let _ = handle_key(&mut app, press(KeyCode::Char('b'), KeyModifiers::NONE));
        assert_eq!(app.mode, ViewMode::Tree);

        let _ = handle_key(&mut app, leader());
        let _ = handle_key(&mut app, press(KeyCode::Char('b'), KeyModifiers::NONE));
        assert_eq!(app.mode, ViewMode::Status);
    }

    #[test]
    fn handle_key_tree_right_left_expand_and_collapse() {
        let (dir, path) = crate::test_util::make_repo();
        let root = std::path::Path::new(&path);
        std::fs::create_dir(root.join("sub")).unwrap();
        std::fs::write(root.join("sub").join("f.txt"), "x").unwrap();

        let mut app = app_with_files(vec![]);
        app.repo_path = path.clone();
        app.focus = Focus::FileList;
        app.enter_tree_mode();
        let idx = app
            .tree_view
            .visible_rows()
            .iter()
            .position(|r| r.path == "sub")
            .unwrap();
        app.tree_view.selected = idx;

        // Right expands the directory.
        let _ = handle_key(&mut app, press(KeyCode::Right, KeyModifiers::NONE));
        assert!(
            app.tree_view
                .visible_rows()
                .iter()
                .any(|r| r.path == "sub/f.txt"),
            "Right must expand the selected directory"
        );

        // Left collapses it again.
        let _ = handle_key(&mut app, press(KeyCode::Left, KeyModifiers::NONE));
        assert!(
            !app.tree_view
                .visible_rows()
                .iter()
                .any(|r| r.path == "sub/f.txt"),
            "Left must collapse the expanded directory"
        );
        drop(dir);
    }

    #[test]
    fn handle_key_terminal_ctrl_w_passes_through_to_pty() {
        // Ctrl+W (and friends) are prompt-editing keys that must now reach
        // the running program as control bytes instead of closing the pane.
        let mut app = app_with_terminal_pane();

        let _ = handle_key(&mut app, press(KeyCode::Char('w'), KeyModifiers::CONTROL));

        // Ctrl+W encodes to 0x17 (ETB).
        assert_eq!(backend_payloads(&app), vec![vec![0x17]]);
    }

    #[test]
    fn handle_key_terminal_ctrl_app_keys_all_pass_through() {
        // Every former bare-Ctrl app shortcut now reaches the PTY untouched.
        // Ctrl+F is excluded: it is the default leader, so it is intercepted to
        // arm the prefix rather than passed through (see the bare-Ctrl+F test).
        for (c, byte) in [
            ('t', 0x14u8),
            ('w', 0x17),
            ('q', 0x11),
            ('l', 0x0c),
            ('p', 0x10),
            ('o', 0x0f),
        ] {
            let mut app = app_with_terminal_pane();
            let _ = handle_key(&mut app, press(KeyCode::Char(c), KeyModifiers::CONTROL));
            assert_eq!(
                backend_payloads(&app),
                vec![vec![byte]],
                "ctrl+{c} must pass through to the PTY"
            );
        }
    }

    #[test]
    fn handle_key_overlay_blocks_leader_when_diff_search_active() {
        // While a search overlay is open the leader is typed/consumed by the
        // overlay, never arming the prefix or firing an app command.
        let mut app = app_with_files(vec!["a.rs"]);
        app.focus = Focus::DiffViewer;
        app.diff.start_search();
        assert!(app.diff.search.active);
        let before = app.mode;

        let _ = handle_key(&mut app, leader());
        assert!(!app.prefix_armed(), "leader must not arm behind an overlay");
        let _ = handle_key(&mut app, press(KeyCode::Char('l'), KeyModifiers::NONE));

        assert_eq!(
            app.mode, before,
            "no app command may fire behind an overlay"
        );
        assert!(app.diff.search.active, "diff search must remain open");
    }

    #[test]
    fn dialog_swallows_the_leader_instead_of_arming_the_prefix() {
        let mut ws = workspace_on(&["/a"]);
        ws.start_repo_input();
        ws.repo_input.buf.clear();

        let _ = dispatch_key(&mut ws, leader());

        // The dispatcher gives the dialog every key, so the leader is typed
        // (and rejected as a control char) rather than arming a prefix behind
        // the modal.
        assert!(!ws.active().unwrap().prefix_armed());
        assert!(ws.repo_input.active);
    }

    #[test]
    fn dialog_rejects_command_modifier_chars() {
        let mut ws = workspace_on(&["/a"]);
        ws.start_repo_input();
        ws.repo_input.buf.clear();

        let alt_x = press(KeyCode::Char('x'), KeyModifiers::ALT);
        let _ = dispatch_key(&mut ws, alt_x);

        assert!(ws.repo_input.buf.is_empty());
    }

    #[test]
    fn handle_key_file_search_rejects_command_modifier_chars() {
        let mut app = app_with_files(vec!["a.rs"]);
        app.focus = Focus::FileList;
        app.start_search();

        let ctrl_x = press(KeyCode::Char('x'), KeyModifiers::CONTROL);
        let _ = handle_key(&mut app, ctrl_x);

        assert!(app.status_view.search_query.is_empty());
    }

    #[test]
    fn handle_key_diff_search_rejects_command_modifier_chars() {
        let mut app = app_with_files(vec!["a.rs"]);
        app.focus = Focus::DiffViewer;
        app.diff.start_search();

        let alt_x = press(KeyCode::Char('x'), KeyModifiers::ALT);
        let _ = handle_key(&mut app, alt_x);

        assert!(app.diff.search.query.is_empty());
    }

    #[test]
    fn handle_key_status_search_shortcut_requires_no_command_modifier() {
        let mut app = app_with_files(vec!["a.rs"]);
        app.focus = Focus::FileList;

        let ctrl_slash = press(KeyCode::Char('/'), KeyModifiers::CONTROL);
        let _ = handle_key(&mut app, ctrl_slash);

        assert!(!app.status_view.search_active);
    }

    #[test]
    fn handle_key_diff_file_toggle_requires_no_command_modifier() {
        let mut app = app_with_files(vec!["a.rs"]);
        app.focus = Focus::DiffViewer;

        let alt_v = press(KeyCode::Char('v'), KeyModifiers::ALT);
        let _ = handle_key(&mut app, alt_v);

        assert_eq!(app.diff.view, DiffPaneView::Diff);
    }

    #[test]
    fn handle_key_diff_search_from_split_returns_to_unified_overlay() {
        let mut app = app_with_files(vec!["a.rs"]);
        app.focus = Focus::DiffViewer;
        app.diff.view = DiffPaneView::Split;

        let _ = handle_key(&mut app, press(KeyCode::Char('/'), KeyModifiers::NONE));

        assert_eq!(app.diff.view, DiffPaneView::Diff);
        assert!(app.diff.search.active);
    }

    #[test]
    fn handle_key_diff_next_match_from_split_returns_to_unified_when_query_exists() {
        let mut app = app_with_files(vec!["a.rs"]);
        app.focus = Focus::DiffViewer;
        app.diff.view = DiffPaneView::Split;
        app.diff.search.query.set("needle");

        let _ = handle_key(&mut app, press(KeyCode::Char('n'), KeyModifiers::NONE));

        assert_eq!(app.diff.view, DiffPaneView::Diff);
    }

    #[test]
    fn handle_paste_into_file_search_strips_control_chars() {
        // Regression for e21c449 + 4084760: paste into the file-search
        // overlay drops control characters (newlines, tabs, bells) before
        // appending to the query.
        let mut app = app_with_files(vec!["alpha.rs", "beta.rs"]);
        app.focus = Focus::FileList;
        app.start_search();

        handle_paste(&mut app, "al\nph\ta\x07");

        assert_eq!(app.status_view.search_query.as_str(), "alpha");
    }

    #[test]
    fn handle_paste_into_diff_search_strips_control_chars() {
        let mut app = app_with_files(vec!["alpha.rs"]);
        app.focus = Focus::DiffViewer;
        app.diff.start_search();

        handle_paste(&mut app, "fn\rname\x08");

        assert_eq!(app.diff.search.query.as_str(), "fnname");
    }

    #[test]
    fn paste_into_the_dialog_strips_control_chars() {
        let mut ws = workspace_on(&["/a"]);
        ws.start_repo_input();
        // `start_repo_input` prefills with the active repo path, and
        // `repo_input_push` preserves existing content, so reset first.
        ws.repo_input.buf.clear();

        dispatch_paste(&mut ws, "/tmp\n/repo\x07");

        assert_eq!(ws.repo_input.buf, "/tmp/repo");
    }

    const MOUSE_TEST_SCREEN: Rect = Rect::new(0, 0, 100, 40);

    /// A single-project tab row, matching the app these mouse tests drive.
    /// Tests that specifically exercise tab clicks build their own list.
    fn test_tabs() -> Vec<String> {
        vec![".".to_string()]
    }

    /// A closed dialog to borrow from, so `test_tab_view` can hand out a
    /// `Chrome` without referencing a temporary.
    static CLOSED_DIALOG: std::sync::LazyLock<ui::status_view::RepoInput> =
        std::sync::LazyLock::new(ui::status_view::RepoInput::default);

    fn test_tab_view(paths: &[String]) -> ui::Chrome<'_> {
        ui::Chrome {
            repo_paths: paths,
            active: 0,
            repo_input: &CLOSED_DIALOG,
        }
    }

    fn mouse(kind: MouseEventKind, column: u16, row: u16) -> MouseEvent {
        MouseEvent {
            kind,
            column,
            row,
            modifiers: KeyModifiers::NONE,
        }
    }

    /// A two-pane terminal app plus each pane's content rect under the
    /// standard test screen, so mouse tests can aim events at real geometry.
    fn app_with_two_panes_and_areas() -> (App, Vec<(backend::PaneId, Rect)>) {
        let mut app = app_with_terminal_pane();
        app.terminal.create_pane().unwrap();
        let layout = config::LayoutConfig::default();
        let areas = ui::terminal_content_areas(&app, MOUSE_TEST_SCREEN, &layout);
        assert_eq!(areas.len(), 2);
        (app, areas)
    }

    #[test]
    fn handle_mouse_click_focuses_the_pane_under_the_pointer() {
        let (mut app, areas) = app_with_two_panes_and_areas();
        app.focus = Focus::FileList;
        let (first_id, rect) = areas[0];
        let first_idx = app
            .terminal
            .panes
            .iter()
            .position(|p| p.id == first_id)
            .unwrap();
        assert_ne!(app.terminal.active, first_idx, "click must change focus");

        let kind = MouseEventKind::Down(crossterm::event::MouseButton::Left);
        handle_mouse(
            &mut app,
            test_tab_view(&test_tabs()),
            mouse(kind, rect.x, rect.y),
            MOUSE_TEST_SCREEN,
            &config::LayoutConfig::default(),
        );

        assert_eq!(app.terminal.active, first_idx);
        assert_eq!(app.focus, Focus::Terminal);
        assert!(
            backend_payloads(&app).is_empty(),
            "a plain shell never claimed the mouse, so the click byte stream \
             must stay empty"
        );
    }

    #[test]
    fn handle_mouse_forwards_press_and_release_to_a_mouse_reporting_pane() {
        let (mut app, areas) = app_with_two_panes_and_areas();
        let (id, rect) = areas[0];
        app.terminal
            .emulators
            .get_mut(&id)
            .unwrap()
            .process(b"\x1b[?1000h\x1b[?1006h");

        let layout = config::LayoutConfig::default();
        let down = MouseEventKind::Down(crossterm::event::MouseButton::Left);
        let up = MouseEventKind::Up(crossterm::event::MouseButton::Left);
        handle_mouse(
            &mut app,
            test_tab_view(&test_tabs()),
            mouse(down, rect.x, rect.y),
            MOUSE_TEST_SCREEN,
            &layout,
        );
        handle_mouse(
            &mut app,
            test_tab_view(&test_tabs()),
            mouse(up, rect.x, rect.y),
            MOUSE_TEST_SCREEN,
            &layout,
        );

        // The pane's top-left content cell is SGR cell (1, 1).
        assert_eq!(
            backend_payloads(&app),
            vec![b"\x1b[<0;1;1M".to_vec(), b"\x1b[<0;1;1m".to_vec()]
        );
    }

    #[test]
    fn handle_mouse_click_focuses_the_upper_panels() {
        let (mut app, _) = app_with_two_panes_and_areas();
        assert_eq!(app.focus, Focus::Terminal);
        let layout = config::LayoutConfig::default();
        let down = MouseEventKind::Down(crossterm::event::MouseButton::Left);

        // Row 1 is the first body row; x=0 is the list, x=60 the diff.
        handle_mouse(
            &mut app,
            test_tab_view(&test_tabs()),
            mouse(down, 0, 1),
            MOUSE_TEST_SCREEN,
            &layout,
        );
        assert_eq!(app.focus, Focus::FileList);

        handle_mouse(
            &mut app,
            test_tab_view(&test_tabs()),
            mouse(down, 60, 1),
            MOUSE_TEST_SCREEN,
            &layout,
        );
        assert_eq!(app.focus, Focus::DiffViewer);

        assert!(
            backend_payloads(&app).is_empty(),
            "an upper-panel click must not write to any PTY"
        );
    }

    #[test]
    fn handle_mouse_release_follows_the_pressed_pane_when_the_pointer_moves_away() {
        let (mut app, areas) = app_with_two_panes_and_areas();
        let (pressed_id, pressed_rect) = areas[0];
        let (_, other_rect) = areas[1];
        // Only the pressed pane is mouse-aware: any release payload proves
        // routing went to the pressed pane, not the pane under the pointer.
        app.terminal
            .emulators
            .get_mut(&pressed_id)
            .unwrap()
            .process(b"\x1b[?1000h\x1b[?1006h");

        let layout = config::LayoutConfig::default();
        let down = MouseEventKind::Down(crossterm::event::MouseButton::Left);
        let up = MouseEventKind::Up(crossterm::event::MouseButton::Left);
        handle_mouse(
            &mut app,
            test_tab_view(&test_tabs()),
            mouse(down, pressed_rect.x, pressed_rect.y),
            MOUSE_TEST_SCREEN,
            &layout,
        );
        handle_mouse(
            &mut app,
            test_tab_view(&test_tabs()),
            mouse(up, other_rect.x, other_rect.y),
            MOUSE_TEST_SCREEN,
            &layout,
        );

        // The release cell is clamped into the pressed pane's rect.
        let col = other_rect.x.clamp(pressed_rect.x, pressed_rect.right() - 1) - pressed_rect.x + 1;
        let row = other_rect
            .y
            .clamp(pressed_rect.y, pressed_rect.bottom() - 1)
            - pressed_rect.y
            + 1;
        let release = format!("\x1b[<0;{col};{row}m").into_bytes();
        assert_eq!(
            backend_payloads(&app),
            vec![b"\x1b[<0;1;1M".to_vec(), release]
        );
        assert!(app.pending_mouse_press.is_none());
    }

    #[test]
    fn handle_mouse_completes_a_pending_release_even_while_the_repo_modal_is_open() {
        let (mut app, areas) = app_with_two_panes_and_areas();
        let (id, rect) = areas[0];
        app.terminal
            .emulators
            .get_mut(&id)
            .unwrap()
            .process(b"\x1b[?1000h\x1b[?1006h");
        let layout = config::LayoutConfig::default();
        let down = MouseEventKind::Down(crossterm::event::MouseButton::Left);
        let up = MouseEventKind::Up(crossterm::event::MouseButton::Left);
        handle_mouse(
            &mut app,
            test_tab_view(&test_tabs()),
            mouse(down, rect.x, rect.y),
            MOUSE_TEST_SCREEN,
            &layout,
        );

        // A release must reach the pane that saw the press even when a modal
        // opened in between, and the pending slot must not go stale. The
        // release path runs before any modal guard, so driving it directly is
        // the same code path a real dialog would take.
        handle_mouse(
            &mut app,
            test_tab_view(&test_tabs()),
            mouse(up, rect.x, rect.y),
            MOUSE_TEST_SCREEN,
            &layout,
        );

        assert_eq!(
            backend_payloads(&app),
            vec![b"\x1b[<0;1;1M".to_vec(), b"\x1b[<0;1;1m".to_vec()]
        );
        assert!(app.pending_mouse_press.is_none());
    }

    #[test]
    fn handle_mouse_release_pairs_by_the_stored_press_button() {
        let (mut app, areas) = app_with_two_panes_and_areas();
        let (id, rect) = areas[0];
        app.terminal
            .emulators
            .get_mut(&id)
            .unwrap()
            .process(b"\x1b[?1000h\x1b[?1006h");

        // Press Right, but the terminal reports the release as Left — the
        // legacy encodings don't carry the button on release, so crossterm
        // may fall back to Left. The pane must still see a Right release.
        let layout = config::LayoutConfig::default();
        let down = MouseEventKind::Down(crossterm::event::MouseButton::Right);
        let up = MouseEventKind::Up(crossterm::event::MouseButton::Left);
        handle_mouse(
            &mut app,
            test_tab_view(&test_tabs()),
            mouse(down, rect.x, rect.y),
            MOUSE_TEST_SCREEN,
            &layout,
        );
        handle_mouse(
            &mut app,
            test_tab_view(&test_tabs()),
            mouse(up, rect.x, rect.y),
            MOUSE_TEST_SCREEN,
            &layout,
        );

        assert_eq!(
            backend_payloads(&app),
            vec![b"\x1b[<2;1;1M".to_vec(), b"\x1b[<2;1;1m".to_vec()]
        );
        assert!(app.pending_mouse_press.is_none());
    }

    #[test]
    fn handle_mouse_is_inert_while_a_search_overlay_is_open() {
        let (mut app, areas) = app_with_two_panes_and_areas();
        app.focus = Focus::FileList;
        app.status_view.search_active = true;
        let (_, rect) = areas[0];
        let active_before = app.terminal.active;

        let kind = MouseEventKind::Down(crossterm::event::MouseButton::Left);
        handle_mouse(
            &mut app,
            test_tab_view(&test_tabs()),
            mouse(kind, rect.x, rect.y),
            MOUSE_TEST_SCREEN,
            &config::LayoutConfig::default(),
        );

        assert_eq!(
            app.focus,
            Focus::FileList,
            "a search overlay owns the mouse exactly like it owns keys"
        );
        assert_eq!(app.terminal.active, active_before);
        assert!(backend_payloads(&app).is_empty());
    }

    #[test]
    fn handle_mouse_drops_a_release_with_no_pending_press() {
        let (mut app, areas) = app_with_two_panes_and_areas();
        let (id, rect) = areas[0];
        app.terminal
            .emulators
            .get_mut(&id)
            .unwrap()
            .process(b"\x1b[?1000h\x1b[?1006h");

        let up = MouseEventKind::Up(crossterm::event::MouseButton::Left);
        handle_mouse(
            &mut app,
            test_tab_view(&test_tabs()),
            mouse(up, rect.x, rect.y),
            MOUSE_TEST_SCREEN,
            &config::LayoutConfig::default(),
        );

        assert!(
            backend_payloads(&app).is_empty(),
            "a pane must not receive a release it never got a press for"
        );
    }

    #[test]
    fn handle_mouse_ignores_events_outside_pane_content() {
        let (mut app, _) = app_with_two_panes_and_areas();
        app.focus = Focus::FileList;
        let active_before = app.terminal.active;

        let kind = MouseEventKind::Down(crossterm::event::MouseButton::Left);
        // (0, 0) is the upper header row, never pane content.
        handle_mouse(
            &mut app,
            test_tab_view(&test_tabs()),
            mouse(kind, 0, 0),
            MOUSE_TEST_SCREEN,
            &config::LayoutConfig::default(),
        );

        assert_eq!(app.focus, Focus::FileList);
        assert_eq!(app.terminal.active, active_before);
    }

    #[test]
    fn mouse_is_inert_while_the_repo_dialog_is_open() {
        let (app, areas) = app_with_two_panes_and_areas();
        let mut ws = Workspace::new(leader());
        ws.add(app);
        ws.active_mut().unwrap().focus = Focus::FileList;
        ws.start_repo_input();
        let (_, rect) = areas[0];
        let active_before = ws.active().unwrap().terminal.active;

        let kind = MouseEventKind::Down(crossterm::event::MouseButton::Left);
        let tabs = test_tabs();
        dispatch_mouse(
            &mut ws,
            test_tab_view(&tabs),
            mouse(kind, rect.x, rect.y),
            MOUSE_TEST_SCREEN,
            &config::LayoutConfig::default(),
            true,
        );

        let app = ws.active().unwrap();
        assert_eq!(app.focus, Focus::FileList, "a modal owns all input");
        assert_eq!(app.terminal.active, active_before);
    }

    /// Wide screen for hint-row click tests: the longest hint rows overflow
    /// the 100-column mouse screen, and a clipped segment is unclickable by
    /// design — these tests target segments, so give them room.
    const HINT_TEST_SCREEN: Rect = Rect::new(0, 0, 300, 40);

    /// First x column on the hint row that resolves to `want`, scanning with
    /// the same hit-test the mouse handler uses.
    fn hint_x_for(app: &App, want: ui::HintClick) -> u16 {
        let row = HINT_TEST_SCREEN.height - 1;
        (0..HINT_TEST_SCREEN.width)
            .find(|&x| {
                ui::hint_click_at(app, test_tab_view(&[]), HINT_TEST_SCREEN, x, row) == Some(want)
            })
            .expect("expected a clickable hint segment")
    }

    /// First (x, y) cell resolving to tab-click target `want`, scanning with
    /// the same hit-test the mouse handler uses.
    fn tab_xy_for(app: &App, want: usize) -> (u16, u16) {
        let layout = config::LayoutConfig::default();
        for y in 0..MOUSE_TEST_SCREEN.height {
            for x in 0..MOUSE_TEST_SCREEN.width {
                if ui::tab_click_at(app, MOUSE_TEST_SCREEN, &layout, x, y) == Some(want) {
                    return (x, y);
                }
            }
        }
        panic!("expected a tab segment targeting pane {want}");
    }

    #[test]
    fn handle_mouse_tab_click_jumps_to_that_pane() {
        let (mut app, _) = app_with_two_panes_and_areas();
        app.terminal.active = 0;
        app.focus = Focus::FileList;
        let (x, y) = tab_xy_for(&app, 1);

        let down = MouseEventKind::Down(crossterm::event::MouseButton::Left);
        handle_mouse(
            &mut app,
            test_tab_view(&test_tabs()),
            mouse(down, x, y),
            MOUSE_TEST_SCREEN,
            &config::LayoutConfig::default(),
        );

        assert_eq!(app.terminal.active, 1);
        assert_eq!(app.focus, Focus::Terminal);
        assert!(
            backend_payloads(&app).is_empty(),
            "a tab click is UI-only; nothing may reach a PTY"
        );
    }

    #[test]
    fn handle_mouse_tab_click_on_hidden_marker_slides_the_window() {
        let mut app = app_with_terminal_pane();
        for _ in 0..5 {
            app.terminal.create_pane().unwrap();
        }
        // 6 panes, window of 4: creation leaves pane 5 active, window [2, 6).
        assert_eq!(app.terminal.visible_start, 2);
        // The left ` +2 ` marker targets the nearest hidden pane, index 1.
        let (x, y) = tab_xy_for(&app, 1);

        let down = MouseEventKind::Down(crossterm::event::MouseButton::Left);
        handle_mouse(
            &mut app,
            test_tab_view(&test_tabs()),
            mouse(down, x, y),
            MOUSE_TEST_SCREEN,
            &config::LayoutConfig::default(),
        );

        assert_eq!(app.terminal.active, 1);
        assert_eq!(
            app.terminal.visible_start, 1,
            "revealing the clicked marker's pane must slide the window one slot"
        );
    }

    #[test]
    fn handle_mouse_click_completes_an_armed_swap_with_the_clicked_pane() {
        let (mut app, areas) = app_with_two_panes_and_areas();
        app.terminal.active = 0;
        let first_id = app.terminal.panes[0].id;
        let (clicked_id, rect) = areas[1];
        assert_ne!(clicked_id, first_id);
        app.begin_swap_target();

        let down = MouseEventKind::Down(crossterm::event::MouseButton::Left);
        handle_mouse(
            &mut app,
            test_tab_view(&test_tabs()),
            mouse(down, rect.x, rect.y),
            MOUSE_TEST_SCREEN,
            &config::LayoutConfig::default(),
        );

        // The clicked pane is the swap target, exactly like its digit: the
        // previously active pane moved into the clicked slot and stays active.
        assert!(!app.awaiting_swap_target());
        assert_eq!(app.terminal.panes[1].id, first_id);
        assert_eq!(app.terminal.active, 1);
        assert!(
            backend_payloads(&app).is_empty(),
            "a swap-target click must not be forwarded to any PTY"
        );
    }

    #[test]
    fn handle_mouse_tab_click_completes_an_armed_swap() {
        let (mut app, _) = app_with_two_panes_and_areas();
        app.terminal.active = 0;
        let first_id = app.terminal.panes[0].id;
        app.begin_swap_target();
        let (x, y) = tab_xy_for(&app, 1);

        let down = MouseEventKind::Down(crossterm::event::MouseButton::Left);
        handle_mouse(
            &mut app,
            test_tab_view(&test_tabs()),
            mouse(down, x, y),
            MOUSE_TEST_SCREEN,
            &config::LayoutConfig::default(),
        );

        assert!(!app.awaiting_swap_target());
        assert_eq!(app.terminal.panes[1].id, first_id);
        assert_eq!(app.terminal.active, 1);
    }

    #[test]
    fn handle_mouse_press_elsewhere_cancels_an_armed_swap() {
        let (mut app, _) = app_with_two_panes_and_areas();
        app.terminal.active = 0;
        app.begin_swap_target();
        let order_before: Vec<_> = app.terminal.panes.iter().map(|p| p.id).collect();

        // (0, 0) is the header row: it names no pane, so the press must
        // consume-and-disarm without swapping or moving focus — the same
        // rule as a non-digit key.
        let down = MouseEventKind::Down(crossterm::event::MouseButton::Left);
        handle_mouse(
            &mut app,
            test_tab_view(&test_tabs()),
            mouse(down, 0, 0),
            MOUSE_TEST_SCREEN,
            &config::LayoutConfig::default(),
        );

        assert!(!app.awaiting_swap_target());
        let order_after: Vec<_> = app.terminal.panes.iter().map(|p| p.id).collect();
        assert_eq!(order_before, order_after);
        assert_eq!(app.terminal.active, 0);
    }

    #[test]
    fn handle_mouse_hint_click_runs_the_named_leader_command() {
        let mut app = app_with_terminal_pane();
        let panes_before = app.terminal.panes.len();
        let x = hint_x_for(&app, ui::HintClick::Leader('t'));

        let down = MouseEventKind::Down(crossterm::event::MouseButton::Left);
        let outcome = handle_mouse(
            &mut app,
            test_tab_view(&test_tabs()),
            mouse(down, x, HINT_TEST_SCREEN.height - 1),
            HINT_TEST_SCREEN,
            &config::LayoutConfig::default(),
        );

        assert!(matches!(outcome, KeyOutcome::Continue));
        assert_eq!(
            app.terminal.panes.len(),
            panes_before + 1,
            "clicking `<prefix> t: new pane` must run the same command as the keys"
        );
        assert!(
            !app.prefix_armed(),
            "the synthesized prefix must not linger"
        );
    }

    #[test]
    fn handle_mouse_hint_click_on_the_leader_label_arms_the_prefix() {
        let mut app = app_with_terminal_pane();
        let x = hint_x_for(&app, ui::HintClick::Arm);

        let down = MouseEventKind::Down(crossterm::event::MouseButton::Left);
        let outcome = handle_mouse(
            &mut app,
            test_tab_view(&test_tabs()),
            mouse(down, x, HINT_TEST_SCREEN.height - 1),
            HINT_TEST_SCREEN,
            &config::LayoutConfig::default(),
        );

        assert!(matches!(outcome, KeyOutcome::Continue));
        assert!(
            app.prefix_armed(),
            "clicking `<prefix>: leader` must arm the prefix exactly like the chord"
        );
        assert!(
            backend_payloads(&app).is_empty(),
            "arming is UI-only; nothing may reach a PTY"
        );
    }

    /// The mouse-only flow the arm click exists for: click the leader label,
    /// then click a follow-up on the armed row.
    #[test]
    fn handle_mouse_arm_click_then_followup_click_runs_the_command() {
        let mut app = app_with_terminal_pane();
        let panes_before = app.terminal.panes.len();
        let down = MouseEventKind::Down(crossterm::event::MouseButton::Left);
        let row = HINT_TEST_SCREEN.height - 1;

        let x = hint_x_for(&app, ui::HintClick::Arm);
        handle_mouse(
            &mut app,
            test_tab_view(&test_tabs()),
            mouse(down, x, row),
            HINT_TEST_SCREEN,
            &config::LayoutConfig::default(),
        );
        let x = hint_x_for(&app, ui::HintClick::Plain('t'));
        handle_mouse(
            &mut app,
            test_tab_view(&test_tabs()),
            mouse(down, x, row),
            HINT_TEST_SCREEN,
            &config::LayoutConfig::default(),
        );

        assert_eq!(
            app.terminal.panes.len(),
            panes_before + 1,
            "arm click + `t` click must open a pane like the key sequence"
        );
        assert!(!app.prefix_armed(), "the follow-up must consume the prefix");
    }

    #[test]
    fn handle_mouse_hint_click_propagates_redraw_from_the_armed_row() {
        let mut app = app_with_terminal_pane();
        app.arm_prefix();
        let x = hint_x_for(&app, ui::HintClick::Plain('r'));

        let down = MouseEventKind::Down(crossterm::event::MouseButton::Left);
        let outcome = handle_mouse(
            &mut app,
            test_tab_view(&test_tabs()),
            mouse(down, x, HINT_TEST_SCREEN.height - 1),
            HINT_TEST_SCREEN,
            &config::LayoutConfig::default(),
        );

        assert!(matches!(outcome, KeyOutcome::Redraw));
        assert!(!app.prefix_armed(), "the follow-up must consume the prefix");
    }

    #[test]
    fn handle_mouse_hint_click_never_quits() {
        let app = app_with_terminal_pane();
        let row = HINT_TEST_SCREEN.height - 1;
        for x in 0..HINT_TEST_SCREEN.width {
            let click = ui::hint_click_at(&app, test_tab_view(&[]), HINT_TEST_SCREEN, x, row);
            assert!(
                !matches!(
                    click,
                    Some(ui::HintClick::Leader('q')) | Some(ui::HintClick::Plain('q'))
                ),
                "x={x} resolves to a quit click"
            );
        }
    }

    #[test]
    fn handle_mouse_wheel_scrolls_the_pane_under_the_pointer_not_the_active_one() {
        let (mut app, areas) = app_with_two_panes_and_areas();
        let (id, rect) = areas[0];
        let idx = app.terminal.panes.iter().position(|p| p.id == id).unwrap();
        let active_before = app.terminal.active;
        assert_ne!(active_before, idx, "wheel must not require focus");
        // Overflow the pane so its emulator has scrollback to move into.
        app.terminal.resize_visible_panes(&[(id, 10, 40)]);
        let output = (0..20).fold(Vec::new(), |mut out, i| {
            out.extend_from_slice(format!("line{i}\r\n").as_bytes());
            out
        });
        app.terminal
            .emulators
            .get_mut(&id)
            .unwrap()
            .process(&output);

        handle_mouse(
            &mut app,
            test_tab_view(&test_tabs()),
            mouse(MouseEventKind::ScrollUp, rect.x, rect.y),
            MOUSE_TEST_SCREEN,
            &config::LayoutConfig::default(),
        );

        assert_eq!(app.terminal.scroll.get(&id).copied(), Some(3));
        assert_eq!(
            app.terminal.active, active_before,
            "a wheel scroll must not steal focus"
        );
    }
}
