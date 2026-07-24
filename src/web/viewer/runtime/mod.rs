//! One background thread per open repository.
//!
//! The thread owns that repository's [`SnapshotChannel`], reduces each snapshot
//! to a wire payload once, and hands it to every subscribed SSE client.
//!
//! `SnapshotChannel` is a single-consumer `mpsc`, so the viewer cannot share
//! the TUI's — it spawns its own. That is the cost of the server never
//! referencing `App`, which is also what lets it run headless.
//!
//! **Fan-out is conflated, not queued.** A subscriber that writes slowly gets
//! the newest status, never a backlog of stale ones: each holds a single slot
//! that the publisher overwrites, plus a one-deep wakeup channel that coalesces.
//! Both are bounded by construction, so a stalled client costs one payload of
//! memory rather than growing without limit.
//!
//! No lock here is ever held across socket I/O: the publisher writes slots and
//! returns, and each client thread takes its update out of its own slot before
//! writing to the network.

use crate::git::diff::RepoSnapshot;
use crate::runtime::snapshot::{SnapshotChannel, SnapshotMsg};
use crate::web::viewer::dto::{Envelope, StatusDto};
use crate::web::viewer::limits;
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc::{self, Receiver, SyncSender, TrySendError};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, SystemTime};

/// How often the runtime thread checks its snapshot channel. The producer ticks
/// about once a second, so this only bounds how late an update is noticed.
const POLL_INTERVAL: Duration = Duration::from_millis(100);

/// One published status, already serialized so N subscribers cost one encode.
#[derive(Debug, Clone)]
pub struct StatusUpdate {
    /// Monotonic per repository. Lets a client tell a replayed snapshot from a
    /// newer one after a reconnect.
    pub seq: u64,
    pub json: Arc<String>,
}

struct Subscriber {
    id: u64,
    /// Latest update, overwritten in place. Holding one value is what makes
    /// this conflated rather than queued.
    slot: Arc<Mutex<Option<StatusUpdate>>>,
    /// One-deep wakeup. A pending token already means "something changed", so
    /// a full channel is success, not backpressure.
    wake: SyncSender<()>,
}

/// A client's handle on the stream. Dropping it unregisters the subscriber, so
/// every exit path — clean close, write error, panic — stops the fan-out.
pub struct Subscription {
    runtime: Arc<RepoRuntime>,
    id: u64,
    slot: Arc<Mutex<Option<StatusUpdate>>>,
    wake_rx: Receiver<()>,
}

impl Subscription {
    /// Wait up to `timeout` for an update, returning the newest one pending.
    ///
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

pub struct RepoRuntime {
    latest: Mutex<Option<StatusUpdate>>,
    subscribers: Mutex<Vec<Subscriber>>,
    next_seq: AtomicU64,
    next_subscriber_id: AtomicU64,
    stop: Arc<AtomicBool>,
    worker: Mutex<Option<thread::JoinHandle<()>>>,
}

impl RepoRuntime {
    /// Start a runtime that watches `repo_path`.
    pub fn spawn(repo_path: &str) -> Arc<Self> {
        let channel = SnapshotChannel::spawn(repo_path);
        Self::start(channel, repo_path.to_string())
    }

    fn start(channel: SnapshotChannel, label: String) -> Arc<Self> {
        let runtime = Arc::new(Self {
            latest: Mutex::new(None),
            subscribers: Mutex::new(Vec::new()),
            next_seq: AtomicU64::new(0),
            next_subscriber_id: AtomicU64::new(0),
            stop: Arc::new(AtomicBool::new(false)),
            worker: Mutex::new(None),
        });

        let worker_runtime = Arc::clone(&runtime);
        let stop = Arc::clone(&runtime.stop);
        let handle = thread::Builder::new()
            .name("nightcrow-viewer-repo".into())
            .spawn(move || {
                while !stop.load(Ordering::Acquire) {
                    // Drain everything queued, then publish only the last one:
                    // intermediate snapshots have already been superseded.
                    let mut newest: Option<(RepoSnapshot, HashMap<String, SystemTime>)> = None;
                    while let Ok(msg) = channel.try_recv() {
                        match msg {
                            SnapshotMsg::Ok(snapshot, mtimes) => newest = Some((snapshot, mtimes)),
                            SnapshotMsg::Err(err) => {
                                tracing::debug!(repo = %label, %err, "viewer: snapshot error")
                            }
                        }
                    }
                    if let Some((snapshot, mtimes)) = newest {
                        worker_runtime.publish(&snapshot, &mtimes);
                    }
                    thread::sleep(POLL_INTERVAL);
                }
                // Dropping the channel here joins the snapshot worker, so the
                // repository handle is released before this thread exits.
                drop(channel);
            })
            .ok();
        *runtime.worker.lock().expect("worker slot poisoned") = handle;
        runtime
    }

    /// Reduce a snapshot to its wire form and hand it to every subscriber.
    fn publish(&self, snapshot: &RepoSnapshot, mtimes: &HashMap<String, SystemTime>) {
        let dto = StatusDto::from_snapshot(
            &snapshot.files,
            snapshot.tracking.as_ref(),
            snapshot.head_oid,
            snapshot.branch_name.as_deref(),
            mtimes,
        );
        let Ok(json) = serde_json::to_string(&Envelope::new(dto)) else {
            tracing::warn!("viewer: status payload failed to serialize");
            return;
        };
        if json.len() > limits::MAX_SSE_PAYLOAD_BYTES {
            // Already capped by the DTO; this only fires if the ceilings drift
            // apart, and dropping beats emitting a payload no client will read.
            tracing::warn!(bytes = json.len(), "viewer: status payload over ceiling");
            return;
        }

        // The snapshot worker ticks on a timer, not on change, so most polls
        // produce a payload identical to the last. Publishing those would keep
        // an idle repository streaming once a second forever and burn a
        // sequence number per tick, making `seq` useless for "did anything
        // happen". Only a real change is an event.
        {
            let latest = self.latest.lock().expect("latest slot poisoned");
            if latest.as_ref().is_some_and(|prev| *prev.json == json) {
                return;
            }
        }

        let update = StatusUpdate {
            seq: self.next_seq.fetch_add(1, Ordering::AcqRel),
            json: Arc::new(json),
        };
        *self.latest.lock().expect("latest slot poisoned") = Some(update.clone());

        let subscribers = self.subscribers.lock().expect("subscribers poisoned");
        for subscriber in subscribers.iter() {
            *subscriber.slot.lock().expect("subscriber slot poisoned") = Some(update.clone());
            // A full wakeup channel already means "go look at your slot".
            match subscriber.wake.try_send(()) {
                Ok(()) | Err(TrySendError::Full(())) => {}
                Err(TrySendError::Disconnected(())) => {
                    tracing::debug!(id = subscriber.id, "viewer: subscriber gone")
                }
            }
        }
    }

    /// Register a client. The subscription starts seeded with the current
    /// status, so a fresh connection renders immediately instead of waiting for
    /// the next change.
    pub fn subscribe(self: &Arc<Self>) -> Subscription {
        let id = self.next_subscriber_id.fetch_add(1, Ordering::AcqRel);
        let seed = self.latest();
        let slot = Arc::new(Mutex::new(seed));
        let (wake, wake_rx) = mpsc::sync_channel(1);

        self.subscribers
            .lock()
            .expect("subscribers poisoned")
            .push(Subscriber {
                id,
                slot: Arc::clone(&slot),
                wake,
            });

        Subscription {
            runtime: Arc::clone(self),
            id,
            slot,
            wake_rx,
        }
    }

    fn unsubscribe(&self, id: u64) {
        self.subscribers
            .lock()
            .expect("subscribers poisoned")
            .retain(|s| s.id != id);
    }

    /// The most recent status, if one has been published.
    pub fn latest(&self) -> Option<StatusUpdate> {
        self.latest.lock().expect("latest slot poisoned").clone()
    }

    pub fn subscriber_count(&self) -> usize {
        self.subscribers.lock().expect("subscribers poisoned").len()
    }

    /// Ask the worker to finish and wait for it. Idempotent.
    pub fn stop(&self) {
        self.stop.store(true, Ordering::Release);
        let handle = self.worker.lock().expect("worker slot poisoned").take();
        if let Some(handle) = handle {
            crate::util::try_timed_join(handle, crate::util::REAP_TIMEOUT);
        }
    }
}

impl Drop for RepoRuntime {
    fn drop(&mut self) {
        self.stop();
    }
}

#[cfg(test)]
mod runtime_tests;