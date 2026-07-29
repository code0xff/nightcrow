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
use crate::runtime::snapshot::{SnapshotChannel, SnapshotMsg, SnapshotWatch};
use crate::web::viewer::dto::{Envelope, StatusDto};
use crate::web::viewer::limits;
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc::{self, TrySendError};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, SystemTime};

mod subscription;

use subscription::Subscriber;
pub use subscription::{StatusUpdate, Subscription};

/// How often the runtime thread checks its snapshot channel. The producer ticks
/// about once a second, so this only bounds how late an update is noticed.
const POLL_INTERVAL: Duration = Duration::from_millis(100);

pub struct RepoRuntime {
    /// The repository this watches, kept so a subscriber arriving while the
    /// watch is paused can be answered with a reading rather than with
    /// whatever was true when the last client left.
    path: String,
    /// Stops and starts the snapshot worker's walking. A `git status` per second
    /// per repository is the daemon's standing cost, and every attached client
    /// pays it again for itself — so the half nobody is reading is not paid.
    watch: SnapshotWatch,
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
        // Asleep from the start, not put to sleep after starting: a repository
        // is opened before anyone looks at it, and the browser subscribes when a
        // page does.
        let channel = SnapshotChannel::spawn_asleep(repo_path);
        Self::start(channel, repo_path.to_string())
    }

    fn start(channel: SnapshotChannel, label: String) -> Arc<Self> {
        let runtime = Arc::new(Self {
            path: label.clone(),
            watch: channel.watch(),
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

        // One hold of `latest` from the check to the fan-out. Three threads
        // publish — the runtime's, a REST handler calling `refresh_now`, and a
        // first subscriber taking its own reading — and split into separate
        // critical sections they interleave: two readings each pass the check,
        // take sequence numbers in one order, and store them in the other, so
        // the older of the two is what `latest` and every subscriber keep. The
        // browser then shows a status that has already been superseded, and
        // `seq` goes backwards under a name that means "newer".
        let mut latest = self.latest.lock().expect("latest slot poisoned");
        // The reader publishes only what changed, but `refresh_now` reads on
        // demand and most of those find nothing new. Publishing them would burn
        // a sequence number apiece, making `seq` useless for "did anything
        // happen". Only a real change is an event.
        if latest.as_ref().is_some_and(|prev| *prev.json == json) {
            return;
        }

        let update = StatusUpdate {
            seq: self.next_seq.fetch_add(1, Ordering::AcqRel),
            json: Arc::new(json),
        };
        *latest = Some(update.clone());

        // Held together with `latest`, and taken in that order everywhere:
        // `latest`, then the subscriber list, then a subscriber's slot.
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
    ///
    /// The first subscriber also starts the watch, and is answered from a reading
    /// taken here rather than from `latest` — while the watch was off, `latest`
    /// is whatever was true when the last client left, which on a page opened
    /// the next morning is not a stale detail but a wrong screen.
    pub fn subscribe(self: &Arc<Self>) -> Subscription {
        let id = self.next_subscriber_id.fetch_add(1, Ordering::AcqRel);
        let slot = Arc::new(Mutex::new(None));
        let (wake, wake_rx) = mpsc::sync_channel(1);

        // Joining the list and starting the watch happen under one hold of the
        // lock, and so does the mirror decision in `unsubscribe`. Counting
        // outside it leaves a gap: the last client of the moment can leave, find
        // the list empty, and stop the reading — after which this subscriber is
        // attached to a repository nobody reads, and nothing but its own
        // disconnection ever starts it again.
        let first = {
            let mut subscribers = self.subscribers.lock().expect("subscribers poisoned");
            let first = subscribers.is_empty();
            subscribers.push(Subscriber {
                id,
                slot: Arc::clone(&slot),
                wake,
            });
            if first {
                self.watch.set_awake(true);
            }
            first
        };
        if first {
            // Outside the lock, which publishing takes.
            self.read_and_publish();
        }
        // Whatever the publish did not leave here: a repository unchanged since
        // the last client left publishes nothing, and this subscriber would
        // render an empty page until something happened. Read before the slot is
        // locked, never while — `publish` holds `latest` and reaches for slots,
        // so taking them the other way round is the two halves of a deadlock.
        let seed = self.latest();
        let mut held = slot.lock().expect("subscriber slot poisoned");
        if held.is_none() {
            *held = seed;
        }
        drop(held);

        Subscription {
            runtime: Arc::clone(self),
            id,
            slot,
            wake_rx,
        }
    }

    fn unsubscribe(&self, id: u64) {
        // Under the same hold of the lock as the removal, for the reason given
        // in `subscribe`: a watch decision taken after letting go of the list can
        // be overtaken by one taken while holding it.
        let mut subscribers = self.subscribers.lock().expect("subscribers poisoned");
        subscribers.retain(|s| s.id != id);
        if subscribers.is_empty() {
            // Nobody is reading, so stop walking the tree. What was published
            // stays in `latest` for anything that asks over REST; the next
            // subscriber replaces it with a reading before it is served (see
            // `subscribe`).
            self.watch.set_awake(false);
        }
    }

    /// Whether the tree is being watched, i.e. whether `latest` is being kept up
    /// to date. False while nothing is subscribed.
    pub fn is_watching(&self) -> bool {
        self.subscriber_count() > 0
    }

    /// Read the repository on this thread and publish it, for a caller that
    /// needs an answer while the watch is off.
    pub fn refresh_now(&self) {
        self.read_and_publish();
    }

    /// Read the repository on this thread and publish it.
    fn read_and_publish(&self) {
        if let Some((snapshot, mtimes)) = SnapshotChannel::read_now(&self.path) {
            self.publish(&snapshot, &mtimes);
        }
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
            crate::platform::threading::try_timed_join(
                handle,
                crate::platform::threading::REAP_TIMEOUT,
            );
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
