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

use crate::cli::{Cli, Commands, run_daemon, run_init};

/// Every path here runs to completion and returns; nothing in this process
/// takes over the terminal. The session runs headless and `attach` is a
/// separate invocation, which is the one that draws.
fn main() -> Result<()> {
    let cli = Cli::parse();
    match cli.command {
        Some(Commands::Init { force }) => run_init(force),
        Some(Commands::Attach) => application::attach::run_attach(),
        Some(Commands::Plugin { command }) => cli::plugin_cmd::run_plugin(command),
        None => run_daemon(cli.exec, cli.port, cli.bind, cli.detach),
    }
}
