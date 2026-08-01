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
pub mod plugin;
mod runtime;
#[cfg(test)]
mod test_util;
mod ui;
mod web;
mod workspace;

use anyhow::Result;
use clap::Parser;

use crate::cli::{Cli, Commands, run_attach_detached, run_daemon, run_init, run_stop};

/// Every path here runs to completion and returns; nothing in this process
/// takes over the terminal. The session runs headless and `attach` is a
/// separate invocation, which is the one that draws.
fn main() -> Result<()> {
    let cli = Cli::parse();
    match cli.command {
        Some(Commands::Init { force }) => run_init(force),
        // `-d` with `attach` means "start one if there isn't one, then attach".
        Some(Commands::Attach) if cli.detach => run_attach_detached(),
        Some(Commands::Attach) => application::attach::run_attach(),
        Some(Commands::Plugin { command }) => cli::plugin_cmd::run_plugin(command),
        Some(Commands::Stop { socket }) => run_stop(socket),
        None => run_daemon(cli.exec, cli.port, cli.bind, cli.detach),
    }
}
