//! The daemon's accept loop: one attached client, two threads. The daemon
//! requires `Hello` as the first frame, then speaks unprompted because the
//! session is shared. An attached client gets a reader (blocked on the socket)
//! and a writer (draining that client's queue); a status query gets neither.
//!
//! Threaded like the viewer's accept loop but with a lower ceiling: a client
//! here is a person at a terminal, not a browser tab.

use anyhow::Context;

use super::clients::AttachedClients;
use super::terminals::TerminalBridges;
use super::transport::UnixListener;
use crate::platform::signals::Shutdown;
use crate::session;
use crate::session::SessionState;
use std::collections::HashMap;
use std::net::SocketAddr;
use std::path::Path;
use std::sync::mpsc::SyncSender;
use std::sync::{Arc, Mutex};

/// Clients that may be attached at once. Each is a person at a terminal, so
/// this is generous for the real case while still bounding a client stuck in a
/// reconnect loop.
pub const MAX_ATTACHED_CLIENTS: usize = 16;
/// Maximum number of sockets waiting for their first protocol frame. Kept as
/// a separate bound from the attached-client registry because one-shot status
/// and stop requests never become attached clients.
pub const MAX_PRE_ATTACH_CONNECTIONS: usize = 16;

mod admission;
mod connection;
mod pre_attach;

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
    pub(super) metadata: super::status::DaemonMetadata,
    admission: Arc<admission::PreAttachAdmission>,
}

impl Session {
    #[cfg(test)]
    pub(super) fn pre_attach_active(&self) -> usize {
        self.admission.active()
    }

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
    attach_endpoint: &Path,
    web_addr: SocketAddr,
    shutdown_tx: SyncSender<Shutdown>,
) -> anyhow::Result<Arc<Session>> {
    let session = Arc::new(Session {
        state,
        clients: Arc::new(AttachedClients::default()),
        bridges: Mutex::new(HashMap::new()),
        nudge: Arc::new(super::watch::Nudge::default()),
        shutdown_tx,
        metadata: super::status::DaemonMetadata::capture(attach_endpoint, web_addr),
        admission: Arc::new(admission::PreAttachAdmission::new(
            MAX_PRE_ATTACH_CONNECTIONS,
        )),
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
        let Some(permit) = session.admission.try_reserve() else {
            tracing::debug!("daemon: refusing a connection over the pre-attach cap");
            continue;
        };
        let session = Arc::clone(&session);
        let _ = std::thread::Builder::new()
            .name("nightcrow-attach".into())
            .spawn(move || connection::run(stream, &session, permit));
    }
}

#[cfg(test)]
#[path = "serve_tests/mod.rs"]
mod tests;
