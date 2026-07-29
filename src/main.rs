mod app;
mod application;
#[cfg(test)]
#[path = "application/tests/mod.rs"]
mod application_tests;
mod backend;
mod cli;
mod config;
mod daemon;
mod git;
mod input;
mod platform;
mod runtime;
#[cfg(test)]
mod test_util;
mod ui;
mod web;
mod workspace;

use anyhow::Result;
use application::event_loop::{ProjectContext, main_loop};
use application::terminal_guard::TerminalGuard;
use clap::Parser;
use crossterm::event::DisableMouseCapture;
use crossterm::{
    execute,
    terminal::{LeaveAlternateScreen, disable_raw_mode},
};
use ratatui::{Terminal, backend::CrosstermBackend};
use std::io;
use syntect::highlighting::ThemeSet;
use workspace::Workspace;

use crate::application::splash::SplashOutcome;
use crate::cli::{
    Cli, Commands, log_anchor_for, resolve_repo_paths, run_init, run_serve, start_viewer_if_enabled,
};

fn main() -> Result<()> {
    let cli = Cli::parse();

    // Subcommands run to completion and exit before any TUI setup, so their
    // output stays on the normal terminal rather than flashing behind the
    // alternate screen.
    match cli.command {
        Some(Commands::Init { force }) => return run_init(force),
        Some(Commands::Serve { repo, port, bind }) => return run_serve(repo, port, bind),
        Some(Commands::Attach { repo }) => return application::attach::run_attach(repo),
        None => {}
    }

    let mut cfg = config::load_config()?;
    // Resolve before entering the alternate screen so a too-many-panes error
    // surfaces as plain stderr text rather than a flash behind the TUI.
    let startup_commands = config::resolve_startup_commands(&cfg, &cli.exec)?;
    // Parse the leader before the alternate screen too, so a malformed
    // `[input] leader` is reported as plain stderr. `load_config` already
    // validated it; re-parsing keeps the KeyEvent local to the app setup.
    let leader = config::parse_leader(&cfg.input.leader)?;

    let repo_paths = resolve_repo_paths(cli.repo)?;

    // The viewer needs the resolved repository list, so it starts after it is
    // built — still before the alternate screen, so its generated password and
    // any bind error stay readable on stderr.
    let viewer = start_viewer_if_enabled(&mut cfg, &repo_paths)?;

    // Logs live under a repo by default, so with no project the first one
    // named on the command line stands in; with none at all, the working
    // directory does. A log path cannot follow the active tab — the file is
    // opened once, at startup.
    let log_anchor = log_anchor_for(&repo_paths)?;
    let _log_guard = platform::logging::init_logging(&cfg.log, &log_anchor);

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
        application::session_link::SessionLink::Local {
            viewer,
            served: Vec::new(),
        },
    )
}

fn run(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    repo_paths: Vec<String>,
    cfg: config::Config,
    startup_commands: Vec<config::StartupCommand>,
    leader: crossterm::event::KeyEvent,
    link: application::session_link::SessionLink,
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
    let stored = workspace::persistence::load_workspace();
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
        ws.add(application::bootstrap::init_app(
            path,
            &cfg,
            &startup_commands,
            leader,
            saved,
        ));
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
        application::splash::splash_loop(terminal, &ws, cfg.theme.preset_index())?,
        SplashOutcome::Quit
    ) {
        tracing::info!("nightcrow stopped during splash");
        return Ok(());
    }
    main_loop(terminal, &mut ws, &ss, &ts, &cfg, &ctx, link)?;

    // Every open project gets its session written, not just the active one:
    // sessions are stored per repo (`<repo>/.nightcrow/session.json`), so a
    // background project's pane/focus state would otherwise be lost purely
    // because the user happened to quit from another tab.
    workspace::persistence::save_workspace(&ws.to_persisted());
    tracing::info!("nightcrow stopped");
    Ok(())
}
