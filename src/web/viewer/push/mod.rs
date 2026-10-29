//! Web Push: a pane asking for its person reaches a browser that is closed.
//!
//! The viewer page subscribes through its service worker and hands the
//! subscription here; the session's [`Notices`](crate::session::notice::Notices)
//! are turned into encrypted messages and sent from one background thread, so
//! a hub's worker never waits on the network.

mod crypto;
mod sender;
mod store;
mod vapid;

pub use store::Subscription;

use crate::session::SessionState;
use crate::session::notice::Notice;
use anyhow::Result;
use serde::Serialize;
use std::path::PathBuf;
use std::sync::mpsc::{SyncSender, TrySendError};
use std::sync::{Arc, Weak};

/// Notices waiting to be sent. A burst past this is dropped, not queued
/// behind a slow push service.
const QUEUE_DEPTH: usize = 64;

/// What the service worker receives, decrypted, for one notification.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Payload {
    pub title: String,
    pub body: String,
    /// One per pane, so a newer notice replaces an older one on the device.
    pub tag: String,
    /// The repository to open when the notification is tapped, if any.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub repo: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pane: Option<u32>,
}

enum Job {
    /// To every subscription, at most once per tag per coalescing window.
    Broadcast(Payload),
    /// To one subscription, now: the settings menu's test button.
    Test { endpoint: String, payload: Payload },
}

struct Shared {
    vapid: vapid::Vapid,
    store: store::Store,
}

pub struct PushService {
    shared: Arc<Shared>,
    jobs: SyncSender<Job>,
}

impl PushService {
    /// Load or make the key in `dir` and start the sender. `None` keeps
    /// everything in memory, for tests: a fresh key and no saved subscriptions.
    pub fn start(dir: Option<PathBuf>) -> Result<Arc<Self>> {
        let vapid = match &dir {
            Some(dir) => vapid::Vapid::load_or_create(&dir.join("vapid"))?,
            None => vapid::Vapid::from_secret(&crypto::random_secret_key()?),
        };
        let store = store::Store::open(dir.map(|dir| dir.join("subscriptions.json")));
        let shared = Arc::new(Shared { vapid, store });
        let (jobs, rx) = std::sync::mpsc::sync_channel(QUEUE_DEPTH);
        let worker = Arc::clone(&shared);
        std::thread::Builder::new()
            .name("nightcrow-push".into())
            .spawn(move || sender::run(&worker, rx))?;
        Ok(Arc::new(Self { shared, jobs }))
    }

    /// `~/.nightcrow/push`, where the key and subscriptions live.
    pub fn default_dir() -> PathBuf {
        PathBuf::from(crate::platform::paths::state_dir_anchor())
            .join(".nightcrow")
            .join("push")
    }

    /// Send each of `session`'s notices. Holds the session weakly: the
    /// session owns the listener list this closure goes into.
    pub fn follow(self: &Arc<Self>, session: &Arc<SessionState>) {
        let push = Arc::clone(self);
        let session_ref: Weak<SessionState> = Arc::downgrade(session);
        session.catalog().notices().subscribe(move |notice| {
            if let Some(session) = session_ref.upgrade() {
                push.broadcast(payload_for(&session, notice));
            }
        });
    }

    pub fn public_key(&self) -> &str {
        self.shared.vapid.public_key()
    }

    /// Keep `subscription`, or say why it cannot be used.
    pub fn subscribe(&self, subscription: Subscription) -> Result<(), &'static str> {
        if let Some(problem) = subscription.problem() {
            return Err(problem);
        }
        self.shared.store.add(subscription);
        Ok(())
    }

    /// Forget `endpoint`. `false` when it was not subscribed.
    pub fn unsubscribe(&self, endpoint: &str) -> bool {
        self.shared.store.remove(endpoint)
    }

    pub fn is_subscribed(&self, endpoint: &str) -> bool {
        self.shared.store.contains(endpoint)
    }

    /// Queue a test notification to `endpoint`. `false` when it is not
    /// subscribed or the queue is full.
    pub fn send_test(&self, endpoint: &str) -> bool {
        if !self.is_subscribed(endpoint) {
            return false;
        }
        let payload = Payload {
            title: "nightcrow".into(),
            body: "Notifications are working.".into(),
            tag: "nightcrow-test".into(),
            repo: None,
            pane: None,
        };
        self.queue(Job::Test {
            endpoint: endpoint.to_string(),
            payload,
        })
    }

    pub fn broadcast(&self, payload: Payload) {
        self.queue(Job::Broadcast(payload));
    }

    fn queue(&self, job: Job) -> bool {
        match self.jobs.try_send(job) {
            Ok(()) => true,
            Err(TrySendError::Full(_)) => {
                tracing::warn!("push: queue full; a notification was dropped");
                false
            }
            Err(TrySendError::Disconnected(_)) => false,
        }
    }
}

/// The notification for `notice`, labelled with what a person recognises.
pub(crate) fn payload_for(session: &SessionState, notice: &Notice) -> Payload {
    let repo = session.catalog().id_of_path(&notice.repo_path);
    let name = repo
        .as_deref()
        .and_then(|id| session.catalog().get(id))
        .map(|entry| entry.name.clone())
        .unwrap_or_else(|| notice.repo_path.clone());
    let pane_label = notice
        .pane_title
        .clone()
        .unwrap_or_else(|| format!("terminal {}", notice.pane));
    let body = match &notice.message {
        Some(message) => format!("{pane_label}: {message}"),
        None => format!("{pane_label} wants your attention"),
    };
    Payload {
        title: name,
        body,
        tag: format!(
            "{}:{}",
            repo.as_deref().unwrap_or(&notice.repo_path),
            notice.pane
        ),
        repo,
        pane: Some(notice.pane),
    }
}

#[cfg(test)]
#[path = "mod_tests.rs"]
mod tests;
