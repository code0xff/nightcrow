//! A nightcrow plugin that notices a coding CLI stopped because it hit its
//! plan's usage limit, waits for the limit to reset, and resumes the exact
//! session it was in.
//!
//! With no subcommand this is the plugin itself: NDJSON on stdin and stdout,
//! spoken to nightcrow. The subcommands are the parts a provider invokes or a
//! human runs once — see each one's help text.
//!
//! What this program will not do, by construction: name the program a pane runs
//! (the host owns that), alter a CLI's permission flags (only resume arguments
//! are ever passed), or write anything down beyond the recovery metadata it
//! needs while it is running.

mod helper;
mod hooks;
mod ipc;
mod protocol;
mod provider;
mod runloop;
mod state;
mod wait;

use clap::{Parser, Subcommand};
use std::process::ExitCode;

#[derive(Debug, Parser)]
#[command(name = "nightcrow-recovery", version, about, long_about = None)]
struct Cli {
    #[command(subcommand)]
    mode: Option<Mode>,
}

#[derive(Debug, Subcommand)]
enum Mode {
    /// Add the Claude Code StopFailure hook and statusline entries to
    /// ~/.claude/settings.json, merging into whatever is already there.
    InstallHooks,
    /// Remove only the entries install-hooks added.
    UninstallHooks,
    /// Internal: the command Claude Code runs for StopFailure. Reads the hook
    /// payload on stdin and forwards a few fields to the running plugin.
    Hook,
    /// Internal: the command Claude Code runs for its statusline. Forwards the
    /// usage windows to the running plugin and prints a short line.
    Statusline,
}

fn main() -> ExitCode {
    match Cli::parse().mode {
        None => report(runloop::run()),
        Some(Mode::InstallHooks) => report(install()),
        Some(Mode::UninstallHooks) => report(uninstall()),
        Some(Mode::Hook) => helper::hook(),
        Some(Mode::Statusline) => helper::statusline(),
    }
}

fn install() -> anyhow::Result<()> {
    let paths = hooks::SettingsPaths::discover()?;
    print_changes(hooks::install(&paths, &current_exe()?)?);
    Ok(())
}

fn uninstall() -> anyhow::Result<()> {
    let paths = hooks::SettingsPaths::discover()?;
    print_changes(hooks::uninstall(&paths)?);
    Ok(())
}

fn print_changes(changes: Vec<String>) {
    for change in changes {
        println!("{change}");
    }
}

/// The absolute path to write into the provider's settings.
///
/// Resolved rather than taken from `argv[0]`: the settings file is read by a
/// different process with a different working directory, so a relative name
/// there would silently stop working.
fn current_exe() -> anyhow::Result<String> {
    let exe = std::env::current_exe()?;
    exe.to_str()
        .map(str::to_string)
        .ok_or_else(|| anyhow::anyhow!("this executable's path is not valid UTF-8: {exe:?}"))
}

/// A human-facing mode's exit status. The message goes to stderr so a mode that
/// also prints data keeps its stdout clean.
fn report(result: anyhow::Result<()>) -> ExitCode {
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("nightcrow-recovery: {e:#}");
            ExitCode::FAILURE
        }
    }
}
