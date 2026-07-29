//! The set of attached clients, and how a change reaches all of them.
//!
//! The session is shared, so a change one client makes is news for every other:
//! a repository opened in the browser has to appear in an attached TUI without
//! it having asked. That rules out plain request/response — the daemon speaks
//! first — so each client gets a queue and a thread that drains it, and state
//! changes are broadcast rather than returned.
//!
//! Refusals are not broadcast. "No such directory" is an answer to the client
//! that asked and noise to everyone else, so it is addressed.

use super::frame::Frame;
use std::sync::Mutex;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{self, Receiver, SyncSender};

/// Frames one client may have queued before it is considered wedged.
///
/// Small on purpose: every message here is either the current repository set —
/// where only the newest matters — or a refusal. A client that cannot keep up
/// with these is not behind, it is stuck, and queueing more would only grow
/// memory on its behalf.
const CLIENT_QUEUE_DEPTH: usize = 32;

struct Attached {
    id: u64,
    tx: SyncSender<Frame>,
}

/// Every client currently attached to the session.
#[derive(Default)]
pub struct AttachedClients {
    inner: Mutex<Vec<Attached>>,
    next_id: AtomicU64,
}

impl AttachedClients {
    /// Register a client, returning its id and the queue its writer drains.
    pub fn connect(&self) -> (u64, Receiver<Frame>) {
        let id = self.next_id.fetch_add(1, Ordering::AcqRel);
        let (tx, rx) = mpsc::sync_channel(CLIENT_QUEUE_DEPTH);
        self.inner
            .lock()
            .expect("attached clients poisoned")
            .push(Attached { id, tx });
        (id, rx)
    }

    /// Forget a client. Dropping its sender ends the writer draining it.
    pub fn disconnect(&self, id: u64) {
        self.inner
            .lock()
            .expect("attached clients poisoned")
            .retain(|client| client.id != id);
    }

    pub fn len(&self) -> usize {
        self.inner.lock().expect("attached clients poisoned").len()
    }

    /// Send `frame` to every attached client.
    ///
    /// Never blocks: the lock is held while queueing, and a blocking send would
    /// let one stalled client stop the session for all the others. A full queue
    /// drops the frame for that client alone.
    pub fn broadcast(&self, frame: Frame) {
        let clients = self.inner.lock().expect("attached clients poisoned");
        for client in clients.iter() {
            if client.tx.try_send(frame.clone()).is_err() {
                tracing::debug!(
                    client = client.id,
                    "daemon: dropping a frame for a full queue"
                );
            }
        }
    }

    /// Send `frame` to one client, if it is still attached.
    pub fn send_to(&self, id: u64, frame: Frame) {
        let clients = self.inner.lock().expect("attached clients poisoned");
        if let Some(client) = clients.iter().find(|client| client.id == id)
            && client.tx.try_send(frame).is_err()
        {
            tracing::debug!(client = id, "daemon: dropping a frame for a full queue");
        }
    }
}

#[cfg(test)]
#[path = "clients_tests.rs"]
mod tests;
