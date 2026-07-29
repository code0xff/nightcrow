//! Waiting for the signals that mean "stop".
//!
//! A headless nightcrow has no keyboard to quit from, so the only way out is a
//! signal: Ctrl-C from the terminal it was started in (SIGINT) or a service
//! manager stopping it (SIGTERM). Both must run the same shutdown, because
//! whichever one arrives, the process owns child shells that need reaping.

use anyhow::{Context, Result};
use signal_hook::consts::signal::{SIGINT, SIGTERM};
use signal_hook::iterator::Signals;

/// A signal that asks the process to stop.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Shutdown {
    /// Ctrl-C in the terminal the process was started from.
    Interrupt,
    /// `kill`, or a service manager stopping the unit.
    Terminate,
}

impl Shutdown {
    pub fn as_str(self) -> &'static str {
        match self {
            Shutdown::Interrupt => "SIGINT",
            Shutdown::Terminate => "SIGTERM",
        }
    }
}

/// Handlers for the stop signals, installed and listening.
///
/// Registering is separate from waiting on purpose. Everything a server does
/// before it is ready to wait — binding, opening repositories, spawning
/// startup shells — takes long enough for a stop signal to arrive in the
/// middle of it, and until a handler is installed such a signal kills the
/// process at its default disposition, skipping the shutdown that reaps the
/// child shells. Register first, and a signal from that moment on is held
/// until [`ShutdownWatch::wait`] collects it.
pub struct ShutdownWatch {
    signals: Signals,
}

impl ShutdownWatch {
    pub fn register() -> Result<Self> {
        let signals =
            Signals::new([SIGINT, SIGTERM]).context("installing SIGINT/SIGTERM handlers")?;
        Ok(Self { signals })
    }

    /// Block until a stop signal has arrived — including one that arrived
    /// before this call, which is held from registration onward.
    ///
    /// Consumes the watch: the handlers come down with it, so a second stop
    /// signal after this returns takes its default disposition. That is
    /// deliberate — Ctrl-C during a shutdown that has itself wedged should
    /// still end the process.
    pub fn wait(mut self) -> Result<Shutdown> {
        for signal in self.signals.forever() {
            match signal {
                SIGINT => return Ok(Shutdown::Interrupt),
                SIGTERM => return Ok(Shutdown::Terminate),
                _ => {}
            }
        }
        // `forever` only ends when the iterator is closed, which nothing here
        // does. Reported rather than silently treated as a stop request.
        anyhow::bail!("signal stream ended without a stop signal")
    }
}

#[cfg(test)]
#[path = "signals_tests.rs"]
mod tests;
