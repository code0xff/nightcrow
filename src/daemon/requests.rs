//! Carrying out one attached client's requests. Refusals go to the asker
//! alone; a client must not flash an error for somebody else's typo. State
//! changes are not answered here at all — the watcher tells every client from
//! one record of what they have been told.

use super::frame::{FrameKind, encode_server, read_frame};
use super::protocol::{ClientMessage, ServerMessage};
use super::serve::Session;
use super::transport::UnixStream;
use crate::session::{self, CloseError, OpenError};
use anyhow::Result;
use std::sync::Arc;

/// Read requests from one client until it detaches.
pub(super) fn read_requests(mut stream: UnixStream, id: u64, session: &Session) -> Result<()> {
    while let Some(frame) = read_frame(&mut stream)? {
        // Terminal frames arrive only once panes are shared; a frame kind with
        // no handler is dropped rather than closing the connection over it.
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
                encode_reply(&ServerMessage::Error {
                    message: format!("unreadable request: {err}"),
                }),
            ),
        }
    }
    Ok(())
}

/// Carry out one request against the served set. A state change is not
/// answered here: the watcher tells every client, from one record of what they
/// have been told (see `watch::watch`). Refusals are addressed to the asker
/// alone.
fn handle(message: ClientMessage, id: u64, session: &Session) {
    let state = &session.state;
    match message {
        ClientMessage::Hello { .. } => {
            refuse(id, session, "hello is only valid as the first request")
        }
        ClientMessage::Status {} => {
            refuse(id, session, "status is only valid as the first request")
        }
        // Answered to the asker alone (nothing changed), but not from here —
        // the set is sent from one place, in session-change order. This records
        // that the asker is owed one and wakes the watcher.
        ClientMessage::ListRepos => {
            session.clients.owe_set(id);
            changed(session);
        }
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
                // Only way to name an unknown repository is to have raced a
                // close on another client. Answered because the asker is
                // waiting to see that tab come forward.
                refuse(id, session, "unknown repository");
            }
        }
        ClientMessage::ReorderRepos { order } => {
            session::reorder_repos(state, &order);
            changed(session);
        }
        // Waits with the rest rather than painting locally: the accent is the
        // session's, and a client that painted first would flicker — the same
        // flicker the tab switch is written to avoid.
        ClientMessage::SetAccent { accent } => {
            session::set_accent(state, accent);
            changed(session);
        }
        // Answered to the asker alone: nothing a reload does shows up in what
        // the other clients are looking at.
        ClientMessage::ReloadConfig => {
            let reply = match crate::session::reload::reload_config(state) {
                Ok(report) => ServerMessage::Reloaded {
                    summary: report.summary(),
                },
                Err(err) => ServerMessage::Error {
                    // The message names the offending key — that is the whole
                    // value of reporting it.
                    message: err.to_string(),
                },
            };
            session.clients.send_to(id, encode_reply(&reply));
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
        // The daemon runs the same shutdown sequence as SIGINT/SIGTERM — reaping
        // every child shell — and then closes the connection. No reply is sent;
        // the connection closing is the acknowledgment.
        ClientMessage::Shutdown => {
            tracing::info!("daemon: shutdown requested by attached client {id}");
            let _ = session
                .shutdown_tx
                .send(crate::platform::signals::Shutdown::Terminate);
        }
    }
}

/// Wake the watcher so it reads the session at once instead of on its next
/// tick. Reading the session is still the watcher's job — this only wakes it.
fn changed(session: &Session) {
    session.nudge.poke();
}

fn refuse(id: u64, session: &Session, message: &str) {
    session.clients.send_to(
        id,
        encode_reply(&ServerMessage::Error {
            message: message.to_string(),
        }),
    );
}

fn encode_reply(message: &ServerMessage) -> super::frame::Frame {
    encode_server(message, "reply", "reply could not be encoded")
}
