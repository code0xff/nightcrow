//! The daemon's accept loop: one attached client, two threads. A client gets a
//! reader and a writer because the daemon speaks unprompted — the session is
//! shared, so a repository opened in the browser has to reach an attached TUI
//! that never asked. The reader blocks on the socket; the writer drains that
//! client's queue.
//!
//! Sized like the viewer's accept loop and for the same reason — a connection
//! costs threads — but with a much lower ceiling. Clients here are terminals a
//! person is sitting at, not browser tabs.

use anyhow::Context;

use super::clients::AttachedClients;
use super::frame::{Frame, write_frame};
use super::protocol::ServerMessage;
use super::terminals::TerminalBridges;
use super::transport::{UnixListener, UnixStream};
use crate::platform::signals::Shutdown;
use crate::web::viewer::server::ViewerState;
use crate::web::viewer::session;
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
    pub(super) state: Arc<ViewerState>,
    pub(super) clients: Arc<AttachedClients>,
    /// Each attached client's terminal subscriptions. Kept here rather than on
    /// the thread that reads that client's socket, because a repository can
    /// appear for reasons that have nothing to do with any client's connection
    /// — the browser opened it — and it has to start streaming for everyone.
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
    ///
    /// Oldest client first, because subscribing is what takes a repository's
    /// pane sizing (the hub gives it to the newest connection): in ascending id
    /// order the newest client subscribes last, so a repository that has just
    /// appeared is sized by the same client that sizes all the others.
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

/// Serve attached clients until the process ends.
///
/// Takes a *clone* of the listener rather than the [`DaemonSocket`]: the socket
/// owns the unlink and the instance lock, and this loop blocks in `accept`
/// forever, so a socket parked here would be freed by process exit — which runs
/// no destructor. The caller keeps it and drops it on the way out.
///
/// [`DaemonSocket`]: super::socket::DaemonSocket
pub fn start(
    state: Arc<ViewerState>,
    shutdown_tx: SyncSender<Shutdown>,
) -> anyhow::Result<Arc<Session>> {
    let session = Arc::new(Session {
        state,
        clients: Arc::new(AttachedClients::default()),
        bridges: Mutex::new(HashMap::new()),
        nudge: Arc::new(super::watch::Nudge::default()),
        shutdown_tx,
    });
    // The only thing that sends the served set, so there is one record of what
    // clients have been told, one order they are told it in, and a change made
    // through the browser reaches them at all. Started here, where it can be
    // refused, rather than inside the accept loop: a session without a watcher
    // serves clients that never learn what is open.
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
        if session.clients.len() >= MAX_ATTACHED_CLIENTS {
            // Dropped rather than answered: writing a refusal here would let one
            // stalled client hold up every attach behind it.
            tracing::debug!("daemon: refusing an attach over the client cap");
            continue;
        }
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
    // draining. Its own two are blocked in `read` and `write`.
    let Ok(hangup) = stream.try_clone() else {
        tracing::debug!("daemon: could not split an attaching client's socket");
        return;
    };
    let (id, queue) = session.clients.connect(hangup);
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

    // Subscribed before the set can reach this client, so the panes of every
    // open repository are already streaming when it learns the repository
    // exists.
    bridges.lock().expect("client bridges poisoned").follow(
        &session::list_session_repos(&session.state),
        session.state.catalog(),
    );
    // The set itself is not sent from here. This client is registered as owed
    // one (`AttachedClients::connect`) and the watcher answers, which is the
    // whole of why a client's frames arrive in the order the session changed —
    // see `watch::watch`. Woken rather than waited for: the poke is what stops
    // this from sitting behind the tick.
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

/// Encode a message into a control frame.
///
/// Encoding cannot fail for these types — they are plain data with no maps
/// keyed by anything but strings — so a failure would mean a bug in the
/// protocol definitions rather than anything a client did. It becomes an error
/// frame so the client sees *something* instead of a silently dropped reply.
pub(super) fn encode(message: &ServerMessage) -> Frame {
    match serde_json::to_vec(message) {
        Ok(json) => Frame::control(json),
        Err(err) => {
            tracing::error!(%err, "daemon: could not encode a reply");
            Frame::control(br#"{"type":"error","message":"reply could not be encoded"}"#.to_vec())
        }
    }
}

#[cfg(test)]
#[path = "serve_tests/mod.rs"]
mod tests;
