//! Carrying out one attached client's requests.
//!
//! A request is either a question — answered to the asker — or a change to the
//! session, which is not answered here at all: every client is looking at the
//! same session, so the watcher tells them all from one record of what they have
//! been told. Refusals go to the asker alone; a client must not flash an error
//! for somebody else's typo.

use super::frame::{FrameKind, read_frame};
use super::protocol::{ClientMessage, ServerMessage, version};
use super::serve::{Session, encode, repos};
use crate::web::viewer::session::{self, CloseError, OpenError};
use anyhow::Result;
use std::os::unix::net::UnixStream;
use std::sync::Arc;

/// Read requests from one client until it detaches.
pub(super) fn read_requests(mut stream: UnixStream, id: u64, session: &Session) -> Result<()> {
    while let Some(frame) = read_frame(&mut stream)? {
        // Terminal frames arrive once panes are shared; until then a client has
        // no pane to write to, and a frame kind with no handler is dropped
        // rather than closing the connection over it.
        if frame.kind != FrameKind::Control {
            tracing::debug!("daemon: ignoring a terminal frame before panes are shared");
            continue;
        }
        match serde_json::from_slice::<ClientMessage>(&frame.payload) {
            Ok(message) => handle(message, id, session),
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
/// A state change is not answered here at all: every attached client is looking
/// at the same session, and the one that asked has no more claim on the result
/// than the others — so the watcher tells them all, from one record of what they
/// have been told. Refusals are addressed to the asker alone, since a client
/// must not flash an error for somebody else's typo.
fn handle(message: ClientMessage, id: u64, session: &Session) {
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
            Ok(_) => changed(session),
            Err(OpenError::EmptyPath) => refuse(id, session, "a path is required"),
            Err(OpenError::NotADirectory) => refuse(id, session, "no such directory"),
            Err(OpenError::TooMany) => refuse(
                id,
                session,
                "the maximum number of repositories is already open",
            ),
        },
        ClientMessage::CloseRepo { repo } => match session::close_repo(state, &repo) {
            Ok(()) => changed(session),
            Err(CloseError::UnknownRepo) => refuse(id, session, "unknown repository"),
        },
        ClientMessage::FocusRepo { repo } => {
            if session::focus_repo(state, &repo).is_ok() {
                changed(session);
            } else {
                // The only way to name a repository the session does not have is
                // to have raced a close on another client. Answered rather than
                // dropped, because the asker is waiting to see that tab come
                // forward and never will.
                refuse(id, session, "unknown repository");
            }
        }
        ClientMessage::ReorderRepos { order } => {
            session::reorder_repos(state, &order);
            changed(session);
        }
        // Not answered to the asker either, though it is the one thing here a
        // client could paint locally without waiting. It waits with the rest:
        // the accent is the session's, and a client that painted first would be
        // the only one showing the new colour for a tick — the same flicker the
        // tab switch is written to avoid.
        ClientMessage::SetAccent { accent } => {
            session::set_accent(state, accent);
            changed(session);
        }
        // Handed straight to the hub, which answers on the subscription rather
        // than here: a pane it creates is news for every client watching that
        // repository, not a reply owed to this one.
        ClientMessage::Terminal { repo, message } => {
            let bridges = session
                .bridges
                .lock()
                .expect("attach bridges poisoned")
                .get(&id)
                .map(Arc::clone);
            if let Some(bridges) = bridges {
                bridges
                    .lock()
                    .expect("client bridges poisoned")
                    .dispatch(&repo, message);
            }
        }
    }
}

/// Every arm that can have changed the session ends here, so the watcher looks
/// at once instead of on its next tick. Reading the session is still its job —
/// this only wakes it.
fn changed(session: &Session) {
    session.nudge.poke();
}

fn refuse(id: u64, session: &Session, message: &str) {
    session.clients.send_to(
        id,
        encode(&ServerMessage::Error {
            message: message.to_string(),
        }),
    );
}
