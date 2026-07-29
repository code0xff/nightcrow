//! The daemon's accept loop: one attached client, two threads.
//!
//! A client gets a reader and a writer because the daemon speaks unprompted —
//! the session is shared, so a repository opened in the browser has to reach an
//! attached TUI that never asked. The reader blocks on the socket; the writer
//! drains that client's queue.
//!
//! Sized like the viewer's accept loop and for the same reason — a connection
//! costs threads — but with a much lower ceiling. Clients here are terminals a
//! person is sitting at, not browser tabs.

use super::clients::AttachedClients;
use super::frame::{Frame, write_frame};
use super::protocol::{RepoSummary, ServerMessage};
use super::terminals::TerminalBridges;
use crate::web::viewer::server::ViewerState;
use crate::web::viewer::session;
use std::collections::HashMap;
use std::io::Write;
use std::os::unix::net::{UnixListener, UnixStream};
use std::sync::{Arc, Mutex};

/// Clients that may be attached at once.
///
/// Each is a person at a terminal, so this is generous for the real case while
/// still bounding a client stuck in a reconnect loop.
pub const MAX_ATTACHED_CLIENTS: usize = 16;

/// Everything the connection threads share.
pub(super) struct Session {
    pub(super) state: Arc<ViewerState>,
    pub(super) clients: Arc<AttachedClients>,
    /// Each attached client's terminal subscriptions.
    ///
    /// Kept here rather than on the thread that reads that client's socket,
    /// because a repository can appear for reasons that have nothing to do with
    /// any client's connection — the browser opened it — and it has to start
    /// streaming for everyone. That thread is blocked in `read` and cannot act
    /// on it; the watcher can. One lock per client, so following a change for
    /// one never delays another's keystrokes.
    pub(super) bridges: Mutex<HashMap<u64, Arc<Mutex<TerminalBridges>>>>,
    /// Wakes the watcher when a client has just asked for a change, so the
    /// answer does not wait out a poll interval.
    pub(super) nudge: Arc<super::watch::Nudge>,
}

impl Session {
    /// Bring every attached client's subscriptions in line with `repos`.
    ///
    /// Oldest client first, because subscribing is what takes a repository's
    /// pane sizing (the hub gives it to the newest connection): in ascending id
    /// order the newest client subscribes last, so a repository that has just
    /// appeared is sized by the same client that sizes all the others rather
    /// than by whichever one a hash map happened to yield first.
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
pub fn serve(listener: UnixListener, state: Arc<ViewerState>) {
    let session = Arc::new(Session {
        state,
        clients: Arc::new(AttachedClients::default()),
        bridges: Mutex::new(HashMap::new()),
        nudge: Arc::new(super::watch::Nudge::default()),
    });
    // The only thing that broadcasts the served set, so there is one record of
    // what clients have been told — and so a change made through the browser
    // reaches them at all.
    let watched = Arc::clone(&session);
    if let Err(err) = std::thread::Builder::new()
        .name("nightcrow-session-watch".into())
        .spawn(move || {
            super::watch::watch(
                Arc::clone(&watched.state),
                Arc::clone(&watched.clients),
                Arc::clone(&watched.nudge),
                |repos| watched.follow_all(repos),
            )
        })
    {
        // Fatal for the session's shape, not for the connection: without it a
        // client sees its own changes and no one else's, which is worth saying
        // plainly rather than leaving to be discovered.
        tracing::error!(%err, "daemon: no session watcher — changes made elsewhere will not arrive");
    }
    for stream in listener.incoming() {
        let Ok(stream) = stream else { continue };
        if session.clients.len() >= MAX_ATTACHED_CLIENTS {
            // Dropped rather than answered: writing a refusal here would let one
            // stalled client hold up every attach behind it, the same reason the
            // viewer's accept loop closes instead of writing a 503.
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

    // The set as it stands, before this client has asked for anything: it needs
    // the session's shape to render, and asking for what the daemon already
    // knows is a round trip for nothing. Subscribed first, so the panes of every
    // open repository are already streaming when it learns the repository
    // exists. Sent here rather than left to the watcher, which only speaks when
    // something changes — and nothing has.
    bridges.lock().expect("client bridges poisoned").follow(
        &session::list_session_repos(&session.state),
        session.state.catalog(),
    );
    send_current_set(id, session);

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

/// Give one client the session as it stands.
///
/// Built under the client registry rather than before reaching for it, so a
/// broadcast cannot land between reading the session and queueing what was read
/// — which would queue the newer state first and leave this client on the older
/// one for good, the watcher having already recorded that everyone was told. See
/// [`AttachedClients::send_built_to`].
fn send_current_set(id: u64, session: &Session) {
    session
        .clients
        .send_built_to(id, || encode(&repos(&session.state)));
}

pub(super) fn repos(state: &ViewerState) -> ServerMessage {
    ServerMessage::Repos {
        repos: session::list_session_repos(state)
            .into_iter()
            .map(|repo| RepoSummary {
                id: repo.id,
                path: repo.path,
            })
            .collect(),
        active: session::active_repo(state),
        accent: session::accent(state),
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
