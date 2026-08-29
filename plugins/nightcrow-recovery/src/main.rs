//! A nightcrow plugin that notices a coding CLI stopped because it hit its
//! plan's usage limit, waits for the limit to reset, and resumes the exact
//! session it was in.
//!
//! The executable is the plugin itself: NDJSON on stdin and stdout, spoken to
//! nightcrow.
//!
//! What this program will not do, by construction: name the program a pane runs
//! (the host owns that), alter a CLI's permission flags (only resume arguments
//! are ever passed), or write anything down beyond the recovery metadata it
//! needs while it is running.

mod protocol;
mod provider;
mod runloop;
mod runloop_io;
mod state;
mod wait;

use std::process::ExitCode;

fn main() -> ExitCode {
    report(runloop::run())
}

fn report(result: anyhow::Result<()>) -> ExitCode {
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("nightcrow-recovery: {e:#}");
            ExitCode::FAILURE
        }
    }
}
