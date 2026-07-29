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
use super::frame::{Frame, FrameKind, read_frame, write_frame};
use super::protocol::{ClientMessage, RepoSummary, ServerMessage, version};
use super::terminals::TerminalBridges;
use crate::web::viewer::server::ViewerState;
use crate::web::viewer::session::{self, CloseError, OpenError};
use anyhow::Result;
use std::io::Write;
use std::os::unix::net::{UnixListener, UnixStream};
use std::sync::Arc;

/// Clients that may be attached at once.
///
/// Each is a person at a terminal, so this is generous for the real case while
/// still bounding a client stuck in a reconnect loop.
pub const MAX_ATTACHED_CLIENTS: usize = 16;

/// Everything the connection threads share.
struct Session {
    state: Arc<ViewerState>,
    clients: Arc<AttachedClients>,
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
    });
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
    // Subscribed before the set is sent, so the panes of every open repository
    // are already streaming when the client learns the repository exists.
    let mut bridges = TerminalBridges::new(id, Arc::clone(&session.clients));

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
    // knows is a round trip for nothing.
    bridges.follow(
        &session::list_session_repos(&session.state),
        session.state.catalog(),
    );
    session.clients.send_to(id, encode(&repos(&session.state)));

    if let Err(err) = read_requests(stream, id, session, &mut bridges) {
        // Expected on detach: the client closes mid-read. Logged at debug
        // because a person quitting is not a fault.
        tracing::debug!(%err, "daemon: attached client ended");
    }
    // Drops this client's sender, which ends the writer draining it.
    session.clients.disconnect(id);
    if let Ok(writer) = writer {
        crate::platform::threading::try_timed_join(
            writer,
            crate::platform::threading::REAP_TIMEOUT,
        );
    }
}

/// Read requests from one client until it detaches.
fn read_requests(
    mut stream: UnixStream,
    id: u64,
    session: &Session,
    bridges: &mut TerminalBridges,
) -> Result<()> {
    while let Some(frame) = read_frame(&mut stream)? {
        // Terminal frames arrive once panes are shared; until then a client has
        // no pane to write to, and a frame kind with no handler is dropped
        // rather than closing the connection over it.
        if frame.kind != FrameKind::Control {
            tracing::debug!("daemon: ignoring a terminal frame before panes are shared");
            continue;
        }
        match serde_json::from_slice::<ClientMessage>(&frame.payload) {
            Ok(message) => handle(message, id, session, bridges),
            // A request this daemon cannot parse is answered, not fatal: the
            // client stays attached and its next request is still served.
            Err(err) => session.clients.send_to(
                id,
                encode(&ServerMessage::Error {
                    message: format!("unreadable request: {err}"),
                }),
            ),
        }
    }
    Ok(())
}

/// Carry out one request against the served set.
///
/// A state change is broadcast rather than returned: every attached client is
/// looking at the same session, and the one that asked has no more claim on the
/// result than the others. Refusals are addressed to the asker alone.
fn handle(message: ClientMessage, id: u64, session: &Session, bridges: &mut TerminalBridges) {
    let state = &session.state;
    match message {
        ClientMessage::Hello { version: client } => {
            let daemon = version();
            let reply = if client == daemon {
                ServerMessage::Hello {
                    version: daemon,
                    client: id,
                }
            } else {
                // Reported, not refused. The two ship in one binary, so a
                // mismatch means two builds are running at once — worth saying
                // plainly rather than failing with a decode error later.
                ServerMessage::Error {
                    message: format!("client is {client}, daemon is {daemon}"),
                }
            };
            session.clients.send_to(id, encode(&reply));
        }
        // Answered to the asker: nothing changed, so there is nothing to tell
        // the others.
        ClientMessage::ListRepos => session.clients.send_to(id, encode(&repos(state))),
        ClientMessage::OpenRepo { path } => match session::open_repo(state, &path) {
            Ok(_) => session.clients.broadcast(encode(&repos(state))),
            Err(OpenError::EmptyPath) => refuse(id, session, "a path is required"),
            Err(OpenError::NotADirectory) => refuse(id, session, "no such directory"),
            Err(OpenError::TooMany) => refuse(
                id,
                session,
                "the maximum number of repositories is already open",
            ),
        },
        ClientMessage::CloseRepo { repo } => match session::close_repo(state, &repo) {
            Ok(()) => session.clients.broadcast(encode(&repos(state))),
            Err(CloseError::UnknownRepo) => refuse(id, session, "unknown repository"),
        },
        ClientMessage::ReorderRepos { order } => {
            session::reorder_repos(state, &order);
            session.clients.broadcast(encode(&repos(state)));
        }
        // Handed straight to the hub, which answers on the subscription rather
        // than here: a pane it creates is news for every client watching that
        // repository, not a reply owed to this one.
        ClientMessage::Terminal { repo, message } => bridges.dispatch(&repo, message),
    }
    // After the set may have changed: a repository this request opened needs a
    // subscription before its startup terminals are offered, and one it closed
    // has a thread to stop.
    bridges.follow(&session::list_session_repos(state), state.catalog());
}

fn refuse(id: u64, session: &Session, message: &str) {
    session.clients.send_to(
        id,
        encode(&ServerMessage::Error {
            message: message.to_string(),
        }),
    );
}

fn repos(state: &ViewerState) -> ServerMessage {
    ServerMessage::Repos {
        repos: session::list_session_repos(state)
            .into_iter()
            .map(|repo| RepoSummary {
                id: repo.id,
                path: repo.path,
            })
            .collect(),
    }
}

/// Encode a message into a control frame.
///
/// Encoding cannot fail for these types — they are plain data with no maps
/// keyed by anything but strings — so a failure would mean a bug in the
/// protocol definitions rather than anything a client did. It becomes an error
/// frame so the client sees *something* instead of a silently dropped reply.
fn encode(message: &ServerMessage) -> Frame {
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
