//! Telling attached clients about changes nobody on their connection asked for.
//!
//! The session has two front doors. A repository opened in the browser goes
//! through the HTTP handlers, and nothing on an attach socket is woken by it —
//! so a client that asks for nothing would sit on a tab list that quietly went
//! stale, which is the one thing a shared session must not do.
//!
//! This is a thread that re-reads the session on a tick and tells everyone when
//! it differs from what they were last told. Observing rather than being
//! notified, because a notification is something a mutation added later can
//! forget to send, and the failure then looks like this same bug again. The cost
//! is a comparison of a handful of small structs at a rate nobody can see.

use super::clients::AttachedClients;
use super::protocol::{RepoSummary, ServerMessage};
use crate::web::viewer::server::ViewerState;
use crate::web::viewer::session;
use std::sync::Arc;
use std::time::Duration;

/// How often the session is re-read.
///
/// Not a latency budget for anything a client does itself — its own requests are
/// answered on the spot. This bounds only how long a change made *elsewhere*
/// takes to appear, where the alternative it replaces was "never".
const TICK: Duration = Duration::from_millis(150);

/// Watch `state` and broadcast the served set whenever it changes.
///
/// `follow` runs for every client before the set goes out, so a repository that
/// appeared is already streaming its terminals by the time a client is told the
/// tab exists.
pub(super) fn watch(
    state: Arc<ViewerState>,
    clients: Arc<AttachedClients>,
    follow: impl Fn(&[session::SessionRepo]),
) {
    // Seeded with the set as it stands, not with nothing: a client is sent the
    // current set when it attaches, so announcing it again on the first tick
    // would be a message that reports no change — and every client would have to
    // treat the arrival of its own starting state as news.
    let mut told: Vec<RepoSummary> = summarize(&session::list_session_repos(&state));
    loop {
        std::thread::sleep(TICK);
        let repos = session::list_session_repos(&state);
        let current = summarize(&repos);
        if told != current {
            follow(&repos);
            clients.broadcast(encode(&ServerMessage::Repos {
                repos: current.clone(),
            }));
            told = current;
        }
    }
}

fn summarize(repos: &[session::SessionRepo]) -> Vec<RepoSummary> {
    repos
        .iter()
        .map(|repo| RepoSummary {
            id: repo.id.clone(),
            path: repo.path.clone(),
        })
        .collect()
}

fn encode(message: &ServerMessage) -> super::frame::Frame {
    match serde_json::to_vec(message) {
        Ok(json) => super::frame::Frame::control(json),
        Err(err) => {
            tracing::error!(%err, "daemon: could not encode a session change");
            super::frame::Frame::control(
                br#"{"type":"error","message":"session change could not be encoded"}"#.to_vec(),
            )
        }
    }
}
