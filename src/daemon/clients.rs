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
use std::os::unix::net::UnixStream;
use std::sync::Mutex;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{self, Receiver, SyncSender};

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
}

impl Attached {
    /// End this connection because it fell too far behind.
    fn cut_off(&self) {
        tracing::warn!(
            client = self.id,
            "daemon: disconnecting an attached client that stopped keeping up"
        );
        let _ = self.socket.shutdown(std::net::Shutdown::Both);
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
            .push(Attached { id, tx, socket });
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
    /// let one stalled client stop the session for all the others. A client whose
    /// queue is full is cut off instead — for itself alone.
    pub fn broadcast(&self, frame: Frame) {
        let mut clients = self.inner.lock().expect("attached clients poisoned");
        clients.retain(|client| {
            if client.tx.try_send(frame.clone()).is_ok() {
                return true;
            }
            client.cut_off();
            false
        });
    }

    /// Send `frame` to one client, if it is still attached.
    pub fn send_to(&self, id: u64, frame: Frame) {
        let mut clients = self.inner.lock().expect("attached clients poisoned");
        let Some(index) = clients.iter().position(|client| client.id == id) else {
            return;
        };
        if clients[index].tx.try_send(frame).is_err() {
            clients[index].cut_off();
            clients.remove(index);
        }
    }
}

#[cfg(test)]
#[path = "clients_tests.rs"]
mod tests;
