//! The attaching side of the daemon socket.
//!
//! Shaped around the fact that the daemon speaks first. A client cannot sit in
//! a request/response loop — the session changes under it, and terminal output
//! will arrive with nothing having asked — so requests are sent and forgotten,
//! and everything the daemon says lands in a queue the caller drains on its own
//! schedule. That schedule is a TUI frame, which must never block on a socket.

use super::frame::{Frame, FrameKind, read_frame, write_frame};
use super::protocol::{ClientMessage, ServerMessage, version};
use anyhow::{Context, Result, bail};
use std::io::Write;
use std::os::unix::net::UnixStream;
use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::Receiver;
use std::time::Duration;

/// How long the opening handshake waits for the daemon to answer.
///
/// Only the handshake is bounded. After it the connection is event-driven and a
/// quiet daemon is the normal state, so a timeout there would be a disconnect
/// invented out of an idle session.
const HANDSHAKE_TIMEOUT: Duration = Duration::from_secs(5);

/// A connection to the session daemon.
#[derive(Debug)]
pub struct DaemonClient {
    /// The write half. The reader thread owns the other.
    out: UnixStream,
    incoming: Receiver<ServerMessage>,
    /// Cleared by the reader thread when the daemon goes away.
    ///
    /// A separate flag rather than the channel's disconnected state: reading
    /// that means calling `try_recv`, which consumes a message when one is
    /// waiting — so asking whether the daemon is there would throw away what it
    /// last said.
    connected: Arc<AtomicBool>,
}

impl DaemonClient {
    /// Attach to the daemon listening on `path`.
    ///
    /// Completes the version handshake before returning, so a caller that gets
    /// a `DaemonClient` knows it is talking to a daemon of this build. The
    /// repository set the daemon volunteers on attach is not waited for — it is
    /// queued like any other message and drained with the rest.
    pub fn connect(path: &Path) -> Result<Self> {
        let stream = UnixStream::connect(path).with_context(|| {
            format!(
                "no nightcrow daemon on {} — start one with `nightcrow serve`",
                path.display()
            )
        })?;
        let mut reader = stream
            .try_clone()
            .context("splitting the daemon connection")?;
        let mut out = stream;

        send(&mut out, &ClientMessage::Hello { version: version() })?;
        // Bounded only for the handshake; cleared before the reader thread takes
        // over, or an idle session would read as a dead one.
        reader
            .set_read_timeout(Some(HANDSHAKE_TIMEOUT))
            .context("setting the handshake timeout")?;
        let mut queued = Vec::new();
        loop {
            let message = read_message(&mut reader)?
                .context("the daemon closed the connection during the handshake")?;
            match message {
                // The id the daemon hands out with it is read once panes are
                // shared, which is what needs to tell this client's own from
                // another's.
                ServerMessage::Hello {
                    version: daemon, ..
                } => {
                    if daemon != version() {
                        bail!("daemon is {daemon}, this client is {}", version());
                    }
                    break;
                }
                // The daemon volunteers the repository set on attach, so it can
                // arrive before the handshake answer. Kept rather than dropped:
                // it is the state this client is about to render.
                // Terminal traffic can start before the handshake answer, since
                // the daemon subscribes this client's repositories the moment it
                // connects. Kept for the same reason the set is.
                other @ (ServerMessage::Repos { .. } | ServerMessage::Terminal { .. }) => {
                    queued.push(other)
                }
                ServerMessage::Error { message } => bail!("daemon refused the attach: {message}"),
            }
        }
        reader
            .set_read_timeout(None)
            .context("clearing the handshake timeout")?;

        let (tx, incoming) = std::sync::mpsc::channel();
        for message in queued {
            let _ = tx.send(message);
        }
        let connected = Arc::new(AtomicBool::new(true));
        let reader_connected = Arc::clone(&connected);
        std::thread::Builder::new()
            .name("nightcrow-daemon-rx".into())
            .spawn(move || {
                // Ends when the daemon closes or the receiver is dropped.
                while let Ok(Some(message)) = read_message(&mut reader) {
                    if tx.send(message).is_err() {
                        break;
                    }
                }
                reader_connected.store(false, Ordering::Release);
            })
            .context("spawning the daemon reader thread")?;

        Ok(Self {
            out,
            incoming,
            connected,
        })
    }

    /// Ask the daemon to open a repository. The answer arrives as a broadcast.
    pub fn open_repo(&mut self, path: &str) -> Result<()> {
        send(
            &mut self.out,
            &ClientMessage::OpenRepo {
                path: path.to_string(),
            },
        )
    }

    /// Ask the daemon to close a repository, by catalog id.
    pub fn close_repo(&mut self, id: &str) -> Result<()> {
        send(
            &mut self.out,
            &ClientMessage::CloseRepo {
                repo: id.to_string(),
            },
        )
    }

    /// Everything the daemon has said since the last drain.
    ///
    /// Never blocks — this runs on a render tick, where waiting on a socket
    /// would stall the whole interface behind a quiet session.
    pub fn drain(&mut self) -> Vec<ServerMessage> {
        // `try_iter` stops at both empty and disconnected, which is what this
        // wants: a daemon that has gone away still has its last messages
        // delivered, and `is_connected` reports the loss separately.
        self.incoming.try_iter().collect()
    }

    /// Whether the daemon is still there.
    ///
    /// Goes false once the reader thread ends, which happens when the daemon
    /// closes the connection or goes away. Messages it already delivered stay
    /// in the queue and are still drained.
    pub fn is_connected(&self) -> bool {
        self.connected.load(Ordering::Acquire)
    }
}

/// Write one request.
fn send(out: &mut UnixStream, message: &ClientMessage) -> Result<()> {
    let json = serde_json::to_vec(message).context("encoding a daemon request")?;
    write_frame(out, &Frame::control(json))?;
    out.flush().context("flushing a daemon request")
}

/// Read one control message, skipping frame kinds this client has no use for
/// yet. `None` at a clean end of stream.
fn read_message(reader: &mut UnixStream) -> Result<Option<ServerMessage>> {
    loop {
        let Some(frame) = read_frame(reader)? else {
            return Ok(None);
        };
        if frame.kind != FrameKind::Control {
            // Terminal frames start arriving once panes are shared. Ignored
            // rather than fatal so a newer daemon cannot break this client by
            // sending one early.
            continue;
        }
        let message =
            serde_json::from_slice(&frame.payload).context("decoding a message from the daemon")?;
        return Ok(Some(message));
    }
}

#[cfg(test)]
#[path = "client_tests.rs"]
mod tests;
