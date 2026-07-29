//! The daemon's accept loop: one thread per attached client.
//!
//! Sized like the viewer's accept loop and for the same reason — a connection
//! costs a thread — but with a much lower ceiling. Clients here are terminals a
//! person is sitting at, not browser tabs, and a bound that low turns a client
//! that reconnects in a loop into a refusal rather than an unbounded pile of
//! threads.

use super::frame::{Frame, read_frame, write_frame};
use super::protocol::{ClientMessage, RepoSummary, ServerMessage, version};
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

/// Serve attached clients until the process ends.
///
/// Takes a *clone* of the listener rather than the [`DaemonSocket`]: the socket
/// owns the unlink and the instance lock, and this loop blocks in `accept`
/// forever, so a socket parked here would be freed by process exit — which runs
/// no destructor. The caller keeps it and drops it on the way out.
///
/// [`DaemonSocket`]: super::socket::DaemonSocket
pub fn serve(listener: UnixListener, state: Arc<ViewerState>) {
    let connections = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    for stream in listener.incoming() {
        let Ok(stream) = stream else { continue };
        let Some(slot) =
            crate::web::common::conn::ConnectionSlot::acquire(&connections, MAX_ATTACHED_CLIENTS)
        else {
            tracing::debug!("daemon: refusing an attach over the client cap");
            // Dropped rather than answered: writing a refusal here would let one
            // stalled client hold up every attach behind it, the same reason the
            // viewer's accept loop closes instead of writing a 503.
            continue;
        };
        let state = Arc::clone(&state);
        let _ = std::thread::Builder::new()
            .name("nightcrow-attach".into())
            .spawn(move || {
                let _slot = slot;
                if let Err(err) = serve_client(stream, &state) {
                    // Expected on detach: the client closes mid-read. Logged at
                    // debug because a person quitting is not a fault.
                    tracing::debug!(%err, "daemon: attached client ended");
                }
            });
    }
}

/// Read requests from one client until it detaches.
fn serve_client(mut stream: UnixStream, state: &ViewerState) -> Result<()> {
    while let Some(frame) = read_frame(&mut stream)? {
        // Terminal frames arrive once panes are shared; until then a client has
        // no pane to write to, and a frame kind with no handler is dropped
        // rather than closing the connection over it.
        if frame.kind != super::frame::FrameKind::Control {
            tracing::debug!("daemon: ignoring a terminal frame before panes are shared");
            continue;
        }
        let reply = match serde_json::from_slice::<ClientMessage>(&frame.payload) {
            Ok(message) => handle(message, state),
            // A request this daemon cannot parse is answered, not fatal: the
            // client stays attached and its next request is still served.
            Err(err) => ServerMessage::Error {
                message: format!("unreadable request: {err}"),
            },
        };
        let json = serde_json::to_vec(&reply)?;
        write_frame(&mut stream, &Frame::control(json))?;
        stream.flush()?;
    }
    Ok(())
}

/// Carry out one request against the served set.
fn handle(message: ClientMessage, state: &ViewerState) -> ServerMessage {
    match message {
        ClientMessage::Hello { version: client } => {
            let daemon = version();
            if client != daemon {
                // Reported, not refused. The two ship in one binary, so a
                // mismatch means two builds are running at once — worth saying
                // plainly rather than failing with a decode error later.
                return ServerMessage::Error {
                    message: format!("client is {client}, daemon is {daemon}"),
                };
            }
            ServerMessage::Hello { version: daemon }
        }
        ClientMessage::ListRepos => repos(state),
        ClientMessage::OpenRepo { path } => match session::open_repo(state, &path) {
            // The whole set, not just the opened repository: the client renders
            // tabs from it, and another client may have changed it meanwhile.
            Ok(_) => repos(state),
            Err(OpenError::EmptyPath) => error("a path is required"),
            Err(OpenError::NotADirectory) => error("no such directory"),
            Err(OpenError::TooMany) => error("the maximum number of repositories is already open"),
        },
        ClientMessage::CloseRepo { repo } => match session::close_repo(state, &repo) {
            Ok(()) => repos(state),
            Err(CloseError::UnknownRepo) => error("unknown repository"),
        },
        ClientMessage::ReorderRepos { order } => {
            session::reorder_repos(state, &order);
            repos(state)
        }
    }
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

fn error(message: &str) -> ServerMessage {
    ServerMessage::Error {
        message: message.to_string(),
    }
}

#[cfg(test)]
#[path = "serve_tests.rs"]
mod tests;
