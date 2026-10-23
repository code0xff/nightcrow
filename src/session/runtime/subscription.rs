//! A client's handle on one repository's status stream, and what travels on it.
//!
//! Conflated rather than queued: a subscriber holds one slot, overwritten in
//! place. The newest status is a complete picture of the working tree, so an
//! intermediate one has nothing left to say — unlike terminal output, where
//! dropping a frame corrupts everything after it.

use super::RepoRuntime;
use std::sync::mpsc::{Receiver, SyncSender};
use std::sync::{Arc, Mutex};
use std::time::Duration;

/// One published status, already serialized so N subscribers cost one encode.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StatusUpdate {
    pub kind: UpdateKind,
    /// The encoded status for [`UpdateKind::Status`]; the reason, as text, for
    /// [`UpdateKind::Unavailable`].
    pub payload: Arc<String>,
}

/// What a [`StatusUpdate`] reports.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UpdateKind {
    /// The working tree as read.
    Status,
    /// The tree could not be read — most often a directory that is not a git
    /// repository yet. Said rather than left silent, or a client waiting for
    /// its first status waits forever.
    Unavailable,
}

pub(super) struct Subscriber {
    pub(super) id: u64,
    /// Latest update, overwritten in place. Holding one value is what makes
    /// this conflated rather than queued.
    pub(super) slot: Arc<Mutex<Option<StatusUpdate>>>,
    /// One-deep wakeup. A pending token already means "something changed", so
    /// a full channel is success, not backpressure.
    pub(super) wake: SyncSender<()>,
}

/// A client's handle on the stream. Dropping it unregisters the subscriber, so
/// every exit path — clean close, write error, panic — stops the fan-out.
pub struct Subscription {
    pub(super) runtime: Arc<RepoRuntime>,
    pub(super) id: u64,
    pub(super) slot: Arc<Mutex<Option<StatusUpdate>>>,
    pub(super) wake_rx: Receiver<()>,
}

impl Subscription {
    /// Wait up to `timeout` for an update, returning the newest one pending.
    /// `None` means nothing arrived in time — the caller should send a
    /// heartbeat and come back, which is how a dead socket gets noticed.
    pub fn next_update(&self, timeout: Duration) -> Option<StatusUpdate> {
        // Take first: a subscription is seeded with the current status at
        // registration, so the first call returns immediately without waiting.
        if let Some(update) = self.take() {
            return Some(update);
        }
        match self.wake_rx.recv_timeout(timeout) {
            Ok(()) => self.take(),
            Err(_) => None,
        }
    }

    fn take(&self) -> Option<StatusUpdate> {
        self.slot.lock().expect("subscriber slot poisoned").take()
    }
}

impl Drop for Subscription {
    fn drop(&mut self) {
        self.runtime.unsubscribe(self.id);
    }
}
