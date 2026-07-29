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
use std::sync::{Arc, Condvar, Mutex};
use std::time::Duration;

/// How long the watcher waits before re-reading the session unprompted.
///
/// This bounds only changes it cannot be told about — the ones made through the
/// browser's HTTP handlers — where the alternative it replaces was "never". A
/// change asked for on an attach socket wakes it immediately (see [`Nudge`]), so
/// a keystroke never waits on this.
const TICK: Duration = Duration::from_millis(150);

/// A way to tell the watcher not to wait out its tick.
///
/// A client that just asked for something is watching for it to happen, so the
/// answer cannot sit behind a poll interval. The change is still *read* from the
/// session rather than passed through here: this only says "look now", so a
/// handler that forgets to poke costs latency, never correctness.
#[derive(Default)]
pub(super) struct Nudge {
    poked: Mutex<bool>,
    wake: Condvar,
}

impl Nudge {
    /// Wake the watcher now.
    pub(super) fn poke(&self) {
        *self.poked.lock().expect("session nudge poisoned") = true;
        self.wake.notify_all();
    }

    /// Wait for a poke, or `timeout`, whichever comes first.
    fn wait(&self, timeout: Duration) {
        let mut poked = self.poked.lock().expect("session nudge poisoned");
        if !*poked {
            let (guard, _) = self
                .wake
                .wait_timeout(poked, timeout)
                .expect("session nudge poisoned");
            poked = guard;
        }
        *poked = false;
    }
}

/// Watch `state` and tell attached clients the served set: everyone, when it —
/// or which repository the session is focused on, or the accent it is painted in
/// — changes, and whoever is still owed one otherwise.
///
/// **The only place a repository set is sent from.** A client that attaches, or
/// asks for the set outright, is marked as owed one and this is what answers;
/// neither sends its own. That is what makes the order a client sees the order
/// the session changed in — one producer per queue, so there is no pair of
/// frames whose order has to be argued about. Two producers, which is what this
/// replaced, could queue a newer frame ahead of an older one and leave a client
/// on state everyone else had moved off.
///
/// `follow` runs for every client before the set goes out, so a repository that
/// appeared is already streaming its terminals by the time a client is told the
/// tab exists. It runs on an accent change too, where it has nothing to do: it
/// skips repositories already followed, so the alternative — deciding here which
/// kind of change deserves it — would buy a walk over a handful of entries at
/// the price of a branch that can be wrong. The owed-only path does not need it:
/// those clients followed the set when they attached, and it has not changed.
pub(super) fn watch(
    state: Arc<ViewerState>,
    clients: Arc<AttachedClients>,
    nudge: Arc<Nudge>,
    follow: impl Fn(&[session::SessionRepo]),
) {
    // Seeded with the set as it stands, not with nothing: an attaching client is
    // owed its own copy and gets one below, so opening with a broadcast would be
    // a message that reports no change — and every other client would have to
    // treat somebody else's arrival as news.
    let mut told = (
        summarize(&session::list_session_repos(&state)),
        session::active_repo(&state),
        session::accent(&state),
    );
    loop {
        nudge.wait(TICK);
        let repos = session::list_session_repos(&state);
        let current = (
            summarize(&repos),
            session::active_repo(&state),
            session::accent(&state),
        );
        let frame = || {
            encode(&ServerMessage::Repos {
                repos: current.0.clone(),
                active: current.1.clone(),
                accent: current.2,
            })
        };
        if told != current {
            follow(&repos);
            // Counts everyone it reaches as told, in the same lock hold, so a
            // client attaching alongside it is either a recipient or still owed
            // one — never neither.
            clients.broadcast(frame());
            told = current;
        } else {
            // Nothing changed, so this says the same thing again to whoever has
            // not heard it yet: a client that just attached, or one that asked.
            for id in clients.take_owed_sets() {
                clients.send_to(id, frame());
            }
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
