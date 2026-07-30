//! The three pump threads behind a [`PluginHost`](super::host::PluginHost):
//! events out, commands in, and diagnostics off stderr.
//!
//! Every loop here ends on its own when the pipe it holds reaches EOF or the
//! peer it talks to is gone, so shutdown never has to interrupt one — it closes
//! a pipe and reaps.

use super::protocol::{MAX_LINE_BYTES, PluginCommand, decode_command, is_blank_line};
use std::io::{BufRead, BufReader, Write};
use std::process::{ChildStderr, ChildStdin, ChildStdout};
use std::sync::mpsc::{Receiver, SyncSender};

/// What one read off a plugin's stdout produced.
enum Pulled {
    Line(String),
    /// Over [`MAX_LINE_BYTES`]; the rest of the line was thrown away so the
    /// next newline resynchronises the stream.
    TooLong(usize),
    NotUtf8,
    Eof,
}

/// Write encoded event lines to the plugin's stdin until the queue closes.
///
/// Dropping `stdin` on return is what tells the plugin the host is done, so
/// this returning is a meaningful signal and not only a thread ending.
pub(super) fn write_events(mut stdin: ChildStdin, rx: Receiver<String>, name: String) {
    for line in rx {
        let written = stdin
            .write_all(line.as_bytes())
            .and_then(|()| stdin.write_all(b"\n"))
            .and_then(|()| stdin.flush());
        if let Err(error) = written {
            tracing::warn!(plugin = %name, %error, "plugin stdin unusable; event writer stopping");
            return;
        }
    }
}

/// Decode commands off the plugin's stdout until EOF or nobody is listening.
pub(super) fn read_commands(stdout: ChildStdout, tx: SyncSender<PluginCommand>, name: String) {
    let mut reader = BufReader::new(stdout);
    loop {
        match pull_line(&mut reader) {
            Ok(Pulled::Line(line)) => {
                if is_blank_line(&line) {
                    continue;
                }
                match decode_command(&line) {
                    Ok(command) => {
                        if tx.send(command).is_err() {
                            return;
                        }
                    }
                    Err(error) => {
                        tracing::warn!(plugin = %name, %error, "refused a line from plugin")
                    }
                }
            }
            Ok(Pulled::TooLong(bytes)) => tracing::warn!(
                plugin = %name,
                bytes,
                limit = MAX_LINE_BYTES,
                "discarded an over-long line from plugin"
            ),
            Ok(Pulled::NotUtf8) => {
                tracing::warn!(plugin = %name, "discarded a line from plugin that is not UTF-8")
            }
            Ok(Pulled::Eof) => return,
            Err(error) => {
                tracing::warn!(plugin = %name, %error, "plugin stdout unusable; reader stopping");
                return;
            }
        }
    }
}

/// Relay the plugin's stderr into the host's log.
///
/// Drained rather than ignored for two reasons: a plugin that dies needs to be
/// diagnosable, and an undrained stderr pipe fills and blocks the plugin inside
/// its own `write`, which looks exactly like a hang.
pub(super) fn drain_stderr(stderr: ChildStderr, name: String) {
    let mut reader = BufReader::new(stderr);
    loop {
        match pull_line(&mut reader) {
            Ok(Pulled::Line(line)) => {
                if !is_blank_line(&line) {
                    tracing::warn!(plugin = %name, "{line}");
                }
            }
            Ok(Pulled::TooLong(bytes)) => {
                tracing::warn!(plugin = %name, bytes, "discarded an over-long stderr line")
            }
            Ok(Pulled::NotUtf8) => {
                tracing::warn!(plugin = %name, "discarded a stderr line that is not UTF-8")
            }
            Ok(Pulled::Eof) => return,
            Err(error) => {
                tracing::warn!(plugin = %name, %error, "plugin stderr unusable; drain stopping");
                return;
            }
        }
    }
}

/// Read one line, or say why there is none to decode.
fn pull_line(reader: &mut impl BufRead) -> std::io::Result<Pulled> {
    let mut buf = Vec::new();
    let (seen, terminated) = read_capped(reader, &mut buf)?;
    if seen == 0 {
        return Ok(Pulled::Eof);
    }
    let line_bytes = seen - usize::from(terminated);
    if line_bytes > MAX_LINE_BYTES {
        // Everything up to the newline was consumed and only the cap's worth
        // kept, so the stream is already resynchronised.
        return Ok(Pulled::TooLong(line_bytes));
    }
    if buf.last() == Some(&b'\r') {
        buf.pop();
    }
    match String::from_utf8(buf) {
        Ok(line) => Ok(Pulled::Line(line)),
        Err(_) => Ok(Pulled::NotUtf8),
    }
}

/// Consume bytes up to and including the next newline, keeping at most
/// [`MAX_LINE_BYTES`] of them in `buf` (the newline is never kept).
///
/// Returns how many bytes the line actually spanned and whether a newline ended
/// it. The cap is applied while reading rather than left to
/// [`decode_command`]'s own length check: by the time that runs the host has
/// already allocated whatever the plugin chose to send, which is the thing worth
/// preventing.
fn read_capped(reader: &mut impl BufRead, buf: &mut Vec<u8>) -> std::io::Result<(usize, bool)> {
    let mut seen = 0;
    loop {
        let available = match reader.fill_buf() {
            Ok(available) => available,
            Err(e) if e.kind() == std::io::ErrorKind::Interrupted => continue,
            Err(e) => return Err(e),
        };
        if available.is_empty() {
            return Ok((seen, false));
        }
        let (take, terminated) = match available.iter().position(|b| *b == b'\n') {
            Some(at) => (at, true),
            None => (available.len(), false),
        };
        let room = MAX_LINE_BYTES.saturating_sub(buf.len());
        buf.extend_from_slice(&available[..take.min(room)]);
        let consumed = take + usize::from(terminated);
        reader.consume(consumed);
        seen += consumed;
        if terminated {
            return Ok((seen, true));
        }
    }
}
