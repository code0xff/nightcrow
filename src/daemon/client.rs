//! The attaching side of the daemon socket.
//!
//! Shaped around the fact that the daemon speaks first. A client cannot sit in
//! a request/response loop — the session changes under it, and terminal output
//! will arrive with nothing having asked — so requests are sent and forgotten,
//! and everything the daemon says lands in a queue the caller drains on its own
//! schedule. That schedule is a TUI frame, which must never block on a socket.

use super::protocol::{ClientMessage, ServerMessage, version};
use super::terminal_link::{TerminalLink, TerminalRouter};
use super::wire::{Incoming, Writer, pump, read_routed, send};
use anyhow::{Context, Result, bail};
use std::os::unix::net::UnixStream;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::Receiver;
use std::sync::{Arc, Mutex};
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
    out: Writer,
    incoming: Receiver<ServerMessage>,
    /// Terminal traffic, split per repository for the backends that drain it.
    terminals: Arc<TerminalRouter>,
    /// This connection's id at the daemon, from the handshake.
    ///
    /// Handed to each repository's backend, which compares it against the
    /// requester a new pane names — the one way to tell a pane this client
    /// opened from one that appeared because another client did.
    client: u64,
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
        let out: Writer = Arc::new(Mutex::new(stream));
        let terminals = Arc::new(TerminalRouter::default());

        send(&out, &ClientMessage::Hello { version: version() })?;
        // Bounded only for the handshake; cleared before the reader thread takes
        // over, or an idle session would read as a dead one.
        reader
            .set_read_timeout(Some(HANDSHAKE_TIMEOUT))
            .context("setting the handshake timeout")?;
        let mut queued = Vec::new();
        let client = loop {
            let incoming = read_routed(&mut reader, &terminals)?
                .context("the daemon closed the connection during the handshake")?;
            let Incoming::Control(message) = incoming else {
                // Terminal traffic starts before the handshake answer, because
                // the daemon subscribes this client's repositories the moment it
                // connects. Already filed with the router by `read_routed`,
                // which is where the panes it describes will be looked for.
                continue;
            };
            match message {
                ServerMessage::Hello {
                    version: daemon,
                    client,
                } => {
                    if daemon != version() {
                        bail!("daemon is {daemon}, this client is {}", version());
                    }
                    break client;
                }
                // The daemon volunteers the repository set on attach, so it can
                // arrive before the handshake answer. Kept rather than dropped:
                // it is the state this client is about to render.
                other @ (ServerMessage::Repos { .. } | ServerMessage::Terminal { .. }) => {
                    queued.push(other)
                }
                ServerMessage::Error { message } => bail!("daemon refused the attach: {message}"),
            }
        };
        reader
            .set_read_timeout(None)
            .context("clearing the handshake timeout")?;

        let (tx, incoming) = std::sync::mpsc::channel();
        for message in queued {
            let _ = tx.send(message);
        }
        let connected = Arc::new(AtomicBool::new(true));
        let reader_connected = Arc::clone(&connected);
        let reader_terminals = Arc::clone(&terminals);
        std::thread::Builder::new()
            .name("nightcrow-daemon-rx".into())
            .spawn(move || {
                pump(&mut reader, &reader_terminals, &tx);
                reader_connected.store(false, Ordering::Release);
            })
            .context("spawning the daemon reader thread")?;

        Ok(Self {
            out,
            incoming,
            terminals,
            client,
            connected,
        })
    }

    /// One repository's end of this connection, for the backend behind its
    /// terminal panes.
    pub fn terminal_link(&self, repo: &str) -> TerminalLink {
        TerminalLink::new(
            repo,
            Arc::clone(&self.out),
            Arc::clone(&self.terminals),
            self.client,
        )
    }

    /// Drop the terminal inboxes of repositories that are no longer open.
    ///
    /// Called with each set the daemon reports, which is also when the tabs are
    /// reconciled: a repository that closed has no backend left to drain it, and
    /// one this client never opened a tab for has an inbox nothing will.
    pub fn retain_repos(&self, open: &[String]) {
        self.terminals.retain(open);
    }

    /// Ask the daemon to open a repository. The answer arrives as a broadcast.
    pub fn open_repo(&mut self, path: &str) -> Result<()> {
        send(
            &self.out,
            &ClientMessage::OpenRepo {
                path: path.to_string(),
            },
        )
    }

    /// Ask the daemon to put a repository in front, by catalog id. Every client
    /// follows, so the answer arrives as a broadcast like any other change.
    pub fn focus_repo(&mut self, id: &str) -> Result<()> {
        send(
            &self.out,
            &ClientMessage::FocusRepo {
                repo: id.to_string(),
            },
        )
    }

    /// Ask the daemon to close a repository, by catalog id.
    pub fn close_repo(&mut self, id: &str) -> Result<()> {
        send(
            &self.out,
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

#[cfg(test)]
#[path = "client_tests.rs"]
mod tests;
