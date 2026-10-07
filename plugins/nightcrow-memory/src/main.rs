//! Shared notes for the coding agents in one nightcrow project.
//!
//! Each agent's CLI runs `nightcrow-memory mcp` as a local MCP server. The
//! processes never talk to each other: every one opens the same SQLite file,
//! found through the environment nightcrow gives its panes, so what one agent
//! saves another in the same project can search.
//!
//! What this program will not do, by construction: read or type into a pane,
//! reach a project other than the one its pane belongs to, or write anything
//! inside the repository's working tree.

mod export;
mod mcp;
mod paths;
mod render;
mod store;
mod tools;

use anyhow::{Result, bail};
use std::process::ExitCode;
use std::time::{SystemTime, UNIX_EPOCH};

const USAGE: &str = "usage: nightcrow-memory <command>

  mcp      serve the shared-memory tools over stdio (run by an agent's CLI)
  export   print this project's notes as Markdown

Both read the project from the environment of the nightcrow pane they run in.";

fn main() -> ExitCode {
    match run(std::env::args().skip(1).collect()) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("nightcrow-memory: {e:#}");
            ExitCode::FAILURE
        }
    }
}

fn run(args: Vec<String>) -> Result<()> {
    match args
        .iter()
        .map(String::as_str)
        .collect::<Vec<_>>()
        .as_slice()
    {
        ["mcp"] => {
            let open = || store::Store::open(&paths::db_path_from_env()?);
            let token = std::env::var(paths::PANE_TOKEN_ENV).ok();
            mcp::Server::new(open, now_epoch, token)
                .serve(std::io::stdin().lock(), std::io::stdout().lock())
        }
        ["export"] => {
            let store = store::Store::open(&paths::db_path_from_env()?)?;
            export::write(&store, now_epoch(), std::io::stdout().lock())
        }
        ["-h" | "--help" | "help"] => {
            println!("{USAGE}");
            Ok(())
        }
        _ => bail!("{USAGE}"),
    }
}

fn now_epoch() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|since| i64::try_from(since.as_secs()).unwrap_or(i64::MAX))
        // A clock before 1970 has no meaningful age for anything; zero keeps
        // ordering by id intact.
        .unwrap_or(0)
}
