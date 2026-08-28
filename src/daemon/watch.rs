//! Telling attached clients about changes nobody on their connection asked
//! for — a repository opened in the browser wakes nothing on an attach socket.
//! A thread that re-reads the session on a tick and tells everyone when it
//! differs from what they were last told. Observing rather than being
//! notified, because a notification is something a mutation added later can
//! forget to send, and the failure then looks like this same bug again.

use super::clients::AttachedClients;
use super::frame::encode_server;
use super::protocol::{RepoSummary, ServerMessage};
use crate::session;
use crate::session::SessionState;
use std::sync::{Arc, Condvar, Mutex};
use std::time::Duration;

/// How long the watcher waits before re-reading the session unprompted. This
/// bounds only changes it cannot be told about — the ones made through the
/// browser's HTTP handlers — where the alternative it replaces was "never". A
/// change asked for on an attach socket wakes it immediately (see [`Nudge`]).
const TICK: Duration = Duration::from_millis(150);

/// A way to tell the watcher not to wait out its tick. The change is still
/// *read* from the session rather than passed through here: a handler that
/// forgets to poke costs latency, never correctness.
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

/// Watch `state` and tell attached clients the served set — everyone, when it
/// (or the focus, or the accent) changes, and whoever is still owed one
/// otherwise.
///
/// **The only place a repository set is sent from.** One producer per queue is
/// what makes the order a client sees the order the session changed in; two
/// producers could queue a newer frame ahead of an older one.
///
/// `follow` runs for every client before the set goes out, so a repository
/// that appeared is already streaming its terminals by the time a client is
/// told the tab exists.
pub(super) fn watch(
    state: Arc<SessionState>,
    clients: Arc<AttachedClients>,
    nudge: Arc<Nudge>,
    follow: impl Fn(&[session::SessionRepo]),
) {
    // Seeded with the set as it stands, not with nothing: an attaching client
    // gets its own copy below, and opening with a broadcast would make every
    // other client treat somebody else's arrival as news.
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
            encode_server(
                &ServerMessage::Repos {
                    repos: current.0.clone(),
                    active: current.1.clone(),
                    accent: current.2,
                },
                "session change",
                "session change could not be encoded",
            )
        };
        if told != current {
            follow(&repos);
            // Counts everyone it reaches as told, in the same lock hold, so a
            // client attaching alongside it is either a recipient or still owed
            // one — never neither.
            clients.broadcast(frame());
            told = current;
        } else {
            // Nothing changed: say the same thing again to whoever has not
            // heard it yet — a client that just attached or asked.
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
