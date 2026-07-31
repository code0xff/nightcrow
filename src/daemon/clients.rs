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
//!
//! **Nothing here is ever skipped.** These queues carry pane output, and a
//! client that misses one frame of it renders every frame after that from a
//! corrupted stream — there is no re-reading a terminal. So a client that stops
//! keeping up is disconnected, the same trade the hub makes one layer in.

use super::frame::Frame;
use super::transport::UnixStream;
use std::sync::Mutex;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{self, Receiver, SyncSender, TrySendError};

/// Frames one client may have queued before it is considered wedged.
///
/// Matches the hub's own client queue, because since the terminals were shared
/// this carries the same thing: a burst of pane output, where the depth is what
/// absorbs the gap between a hub broadcasting and a socket accepting. Reaching
/// the end of it means the client is not behind but stuck.
const CLIENT_QUEUE_DEPTH: usize = 256;

struct Attached {
    id: u64,
    tx: SyncSender<Frame>,
    /// A third handle on this client's socket, only ever used to end the
    /// connection when its queue overflows.
    ///
    /// Dropping the sender alone would stop the writer thread and leave the
    /// reader blocked, so the client would go quiet while still believing it was
    /// attached. Closing the socket is what turns that into the disconnect it
    /// actually is.
    socket: UnixStream,
    /// Whether this client is still waiting to be told the session's shape.
    ///
    /// Set when it attaches, and when it asks for the set outright. Cleared by
    /// the watcher, which is the only thing that sends one — a client's frames
    /// all coming from one thread is what makes their order the order the
    /// session changed in.
    owed_set: bool,
}

impl Attached {
    /// Queue `frame`, reporting whether this client is still worth keeping.
    ///
    /// A full queue means the client stopped draining: it is cut off, because
    /// the alternative is serving it a stream with a hole in it. A disconnected
    /// one has already gone — its writer thread ended when the socket broke —
    /// so it is simply forgotten, not reported as stalled.
    fn queue(&self, frame: Frame) -> bool {
        match self.tx.try_send(frame) {
            Ok(()) => true,
            Err(TrySendError::Full(_)) => {
                tracing::warn!(
                    client = self.id,
                    "daemon: disconnecting an attached client that stopped keeping up"
                );
                let _ = self.socket.shutdown(std::net::Shutdown::Both);
                false
            }
            Err(TrySendError::Disconnected(_)) => {
                tracing::debug!(client = self.id, "daemon: attached client already gone");
                false
            }
        }
    }
}

/// Every client currently attached to the session.
#[derive(Default)]
pub struct AttachedClients {
    inner: Mutex<Vec<Attached>>,
    next_id: AtomicU64,
}

impl AttachedClients {
    /// Register a client, returning its id and the queue its writer drains.
    ///
    /// `socket` is a handle on the client's connection, used only to close it if
    /// the client stops draining.
    pub fn connect(&self, socket: UnixStream) -> (u64, Receiver<Frame>) {
        let id = self.next_id.fetch_add(1, Ordering::AcqRel);
        let (tx, rx) = mpsc::sync_channel(CLIENT_QUEUE_DEPTH);
        self.inner
            .lock()
            .expect("attached clients poisoned")
            .push(Attached {
                id,
                tx,
                socket,
                // It has nothing on screen yet, and the watcher is what hands
                // the session over. The caller wakes it (`Nudge::poke`) rather
                // than sending anything itself.
                owed_set: true,
            });
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

    /// Send `frame` to every attached client, and count them told: nobody is
    /// left owed a set by a broadcast that just reached them.
    ///
    /// The two are one act, under one lock hold, because a client that attaches
    /// between them was *not* a recipient — clearing its flag afterwards would
    /// leave it waiting for a set the watcher has already recorded as sent, and
    /// with no further change to the session nothing would ever send one. A
    /// client that attaches after this returns is not in the list, keeps its
    /// flag, and is served on the next pass.
    ///
    /// The served set is the only thing every client is sent at once — a
    /// repository's pane output goes per subscriber — so there is no broadcast
    /// this does not settle.
    ///
    /// Never blocks: the lock is held while queueing, and a blocking send would
    /// let one stalled client stop the session for all the others. A client whose
    /// queue is full is cut off instead — for itself alone.
    pub fn broadcast(&self, frame: Frame) {
        let mut clients = self.inner.lock().expect("attached clients poisoned");
        clients.retain_mut(|client| {
            client.owed_set = false;
            client.queue(frame.clone())
        });
    }

    /// Note that `id` is waiting to be told the session's shape, for the watcher
    /// to answer on its next pass. Unknown ids are ignored: the client detached
    /// between asking and this.
    pub fn owe_set(&self, id: u64) {
        let mut clients = self.inner.lock().expect("attached clients poisoned");
        if let Some(client) = clients.iter_mut().find(|client| client.id == id) {
            client.owed_set = true;
        }
    }

    /// Take the ids waiting to be told the session's shape, clearing them.
    ///
    /// Drained rather than read, so a client is owed a set exactly once per
    /// asking — the watcher is about to send it one.
    pub fn take_owed_sets(&self) -> Vec<u64> {
        let mut clients = self.inner.lock().expect("attached clients poisoned");
        clients
            .iter_mut()
            .filter_map(|client| std::mem::take(&mut client.owed_set).then_some(client.id))
            .collect()
    }

    /// Send `frame` to one client, if it is still attached.
    pub fn send_to(&self, id: u64, frame: Frame) {
        let mut clients = self.inner.lock().expect("attached clients poisoned");
        let Some(index) = clients.iter().position(|client| client.id == id) else {
            return;
        };
        if !clients[index].queue(frame) {
            clients.remove(index);
        }
    }
}

#[cfg(test)]
#[path = "clients_tests.rs"]
mod tests;
