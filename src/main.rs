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
mod persistence;
mod platform;
pub mod plugin;
mod runtime;
mod session;
#[cfg(test)]
mod test_util;
mod ui;
mod web;
mod workspace;

use anyhow::Result;
use clap::Parser;

use crate::cli::{
    Cli, Commands, run_attach_detached, run_daemon, run_init, run_status, run_stop, run_update,
};

/// Every path here runs to completion and returns; nothing in this process
/// takes over the terminal. The session runs headless and `attach` is a
/// separate invocation, which is the one that draws.
fn main() -> Result<()> {
    let cli = Cli::parse();
    // Collect binaries an update parked but could not delete while they ran.
    crate::platform::self_replace::sweep_beside_current_exe();
    match cli.command {
        Some(Commands::Init { force }) => run_init(force),
        // Attach starts a session when none is running, with or without `-d`:
        // the first command of the day should not have to be two commands, and
        // a session that has to exist for the TUI to draw is not a choice the
        // user was making. `-d` still says how the session runs — in the
        // background — which is what it already does here.
        Some(Commands::Attach) => run_attach_detached(),
        Some(Commands::Plugin { command }) => cli::plugin_cmd::run_plugin(command),
        Some(Commands::Stop { socket }) => run_stop(socket),
        Some(Commands::Status { socket }) => run_status(socket),
        Some(Commands::Update { version, path, git }) => run_update(version, path, git),
        None => run_daemon(cli.exec, cli.port, cli.bind, cli.detach),
    }
}
