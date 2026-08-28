//! The daemon's accept loop: one attached client, two threads. The daemon
//! speaks unprompted — the session is shared — so a client gets a reader
//! (blocked on the socket) and a writer (draining that client's queue).
//!
//! Threaded like the viewer's accept loop but with a lower ceiling: a client
//! here is a person at a terminal, not a browser tab.

use anyhow::Context;

use super::clients::AttachedClients;
use super::frame::write_frame;
use super::terminals::TerminalBridges;
use super::transport::{UnixListener, UnixStream};
use crate::platform::signals::Shutdown;
use crate::session;
use crate::session::SessionState;
use std::collections::HashMap;
use std::io::Write;
use std::sync::mpsc::SyncSender;
use std::sync::{Arc, Mutex};

/// Clients that may be attached at once. Each is a person at a terminal, so
/// this is generous for the real case while still bounding a client stuck in a
/// reconnect loop.
pub const MAX_ATTACHED_CLIENTS: usize = 16;

/// Everything the connection threads share.
pub struct Session {
    pub(super) state: Arc<SessionState>,
    pub(super) clients: Arc<AttachedClients>,
    /// Each attached client's terminal subscriptions. Kept here rather than on
    /// the client's socket thread: a repository can appear without any client
    /// asking (the browser opened it) and must start streaming for everyone.
    /// One lock per client, so following a change for one never delays
    /// another's keystrokes.
    pub(super) bridges: Mutex<HashMap<u64, Arc<Mutex<TerminalBridges>>>>,
    /// Wakes the watcher when a client has just asked for a change, so the
    /// answer does not wait out a poll interval.
    pub(super) nudge: Arc<super::watch::Nudge>,
    /// Signals the main thread to stop. Sent by the `Shutdown` client message
    /// handler, and also by the signal-forwarding thread in `cli.rs`.
    pub(super) shutdown_tx: SyncSender<Shutdown>,
}

impl Session {
    /// Bring every attached client's subscriptions in line with `repos`.
    /// Oldest client first: subscribing takes a repository's pane sizing (the
    /// hub gives it to the newest connection), so in ascending id order the
    /// newest client subscribes last and sizes a just-appeared repository the
    /// same way it sizes all the others.
    fn follow_all(&self, repos: &[session::SessionRepo]) {
        let mut bridges: Vec<(u64, Arc<Mutex<TerminalBridges>>)> = self
            .bridges
            .lock()
            .expect("attach bridges poisoned")
            .iter()
            .map(|(id, bridges)| (*id, Arc::clone(bridges)))
            .collect();
        bridges.sort_by_key(|(id, _)| *id);
        for (_, client) in bridges {
            client
                .lock()
                .expect("client bridges poisoned")
                .follow(repos, self.state.catalog());
        }
    }
}

/// Serve attached clients until the process ends. Takes a *clone* of the
/// listener rather than the [`DaemonSocket`]: this loop blocks in `accept`
/// forever and process exit runs no destructor, so a socket parked here would
/// be freed without unlinking it or releasing the lock. The caller keeps the
/// socket and drops it on the way out.
///
/// [`DaemonSocket`]: super::socket::DaemonSocket
pub fn start(
    state: Arc<SessionState>,
    shutdown_tx: SyncSender<Shutdown>,
) -> anyhow::Result<Arc<Session>> {
    let session = Arc::new(Session {
        state,
        clients: Arc::new(AttachedClients::default()),
        bridges: Mutex::new(HashMap::new()),
        nudge: Arc::new(super::watch::Nudge::default()),
        shutdown_tx,
    });
    // The only sender of the served set, so clients are told it in one order
    // and changes made through the browser reach them at all. Started outside
    // the accept loop: a session without a watcher serves clients that never
    // learn what is open.
    let watched = Arc::clone(&session);
    std::thread::Builder::new()
        .name("nightcrow-session-watch".into())
        .spawn(move || {
            super::watch::watch(
                Arc::clone(&watched.state),
                Arc::clone(&watched.clients),
                Arc::clone(&watched.nudge),
                |repos| watched.follow_all(repos),
            )
        })
        .context("starting the session watcher")?;
    Ok(session)
}

/// Accept attached clients until the process ends. Takes the session [`start`]
/// prepared.
pub fn serve(listener: UnixListener, session: Arc<Session>) {
    for stream in listener.incoming() {
        let Ok(stream) = stream else { continue };
        let session = Arc::clone(&session);
        let _ = std::thread::Builder::new()
            .name("nightcrow-attach".into())
            .spawn(move || attach(stream, &session));
    }
}

/// Serve one client for as long as it stays attached.
fn attach(stream: UnixStream, session: &Session) {
    let Ok(write_half) = stream.try_clone() else {
        tracing::debug!("daemon: could not split an attaching client's socket");
        return;
    };
    // A third handle, so the set can end this connection if the client stops
    // draining — the other two are blocked in `read` and `write`.
    let Ok(hangup) = stream.try_clone() else {
        tracing::debug!("daemon: could not split an attaching client's socket");
        return;
    };
    let Some((id, queue)) = session.clients.try_connect(hangup, MAX_ATTACHED_CLIENTS) else {
        // Dropped rather than answered: writing a refusal here would let one
        // stalled client hold up every attach behind it.
        tracing::debug!("daemon: refusing an attach over the client cap");
        return;
    };
    let bridges = Arc::new(Mutex::new(TerminalBridges::new(
        id,
        Arc::clone(&session.clients),
    )));
    session
        .bridges
        .lock()
        .expect("attach bridges poisoned")
        .insert(id, Arc::clone(&bridges));

    // The writer owns its half outright, so the reader below can stay blocked
    // in `read` while frames go out.
    let writer = std::thread::Builder::new()
        .name("nightcrow-attach-tx".into())
        .spawn(move || {
            let mut out = write_half;
            for frame in queue {
                if write_frame(&mut out, &frame).is_err() || out.flush().is_err() {
                    break;
                }
            }
        });

    // Subscribed before the watcher can reach this client, so every open
    // repository's panes are already streaming when the client learns the
    // repositories exist.
    bridges.lock().expect("client bridges poisoned").follow(
        &session::list_session_repos(&session.state),
        session.state.catalog(),
    );
    // The set itself is sent by the watcher, to which this client is already
    // registered as owed one — that is what keeps a client's frames in the
    // order the session changed (see `watch::watch`).
    session.nudge.poke();

    if let Err(err) = super::requests::read_requests(stream, id, session) {
        // Expected on detach: the client closes mid-read. Logged at debug
        // because a person quitting is not a fault.
        tracing::debug!(%err, "daemon: attached client ended");
    }
    // Drops this client's sender, which ends the writer draining it, and its
    // subscriptions, which stops the threads relaying its terminals.
    session
        .bridges
        .lock()
        .expect("attach bridges poisoned")
        .remove(&id);
    session.clients.disconnect(id);
    if let Ok(writer) = writer {
        crate::platform::threading::try_timed_join(
            writer,
            crate::platform::threading::REAP_TIMEOUT,
        );
    }
}

#[cfg(test)]
#[path = "serve_tests/mod.rs"]
mod tests;
