//! The plugin's two ends of the host's NDJSON stream.
//!
//! Split out of `runloop.rs` so that file is the loop's reasoning and this one
//! is its plumbing. Everything the plugin says leaves through [`emit`], called
//! only from the main thread, which is what keeps two half-written lines from
//! interleaving on stdout.

use crate::ipc::IpcMessage;
use crate::protocol::{LogLevel, PluginCommand, PluginEvent, decode_event, encode_command, log};
use anyhow::Result;
use std::io::{BufRead, BufReader, Write};
use std::sync::mpsc::Sender;

/// What the main thread waits on.
pub(crate) enum Message {
    Host(PluginEvent),
    /// A line from the host that could not be understood. Reported and skipped:
    /// one bad line is not a reason to abandon a session's panes.
    HostGarbage(String),
    Signal(IpcMessage),
    /// stdin ended. The host is gone, so there is nothing left to serve.
    HostGone,
}

pub(crate) fn spawn_stdin_reader(tx: Sender<Message>) {
    std::thread::spawn(move || {
        let reader = BufReader::new(std::io::stdin());
        for line in reader.lines() {
            let Ok(line) = line else {
                break;
            };
            if line.trim().is_empty() {
                continue;
            }
            let message = match decode_event(&line) {
                Ok(event) => Message::Host(event),
                Err(e) => Message::HostGarbage(format!("ignoring a host line: {e}")),
            };
            if tx.send(message).is_err() {
                return;
            }
        }
        let _ = tx.send(Message::HostGone);
    });
}

pub(crate) fn emit_all(commands: &[PluginCommand]) -> Result<()> {
    for command in commands {
        emit(command)?;
    }
    Ok(())
}

/// Write one command as one NDJSON line. A command that cannot be framed is
/// dropped with a complaint rather than corrupting the stream.
pub(crate) fn emit(command: &PluginCommand) -> Result<()> {
    let line = match encode_command(command) {
        Ok(line) => line,
        Err(e) => encode_command(&log(
            LogLevel::Error,
            format!("dropped an unencodable command: {e}"),
        ))?,
    };
    let mut out = std::io::stdout().lock();
    out.write_all(line.as_bytes())?;
    out.write_all(b"\n")?;
    out.flush()?;
    Ok(())
}
