//! The browsers subscribed to Web Push, kept across daemon restarts.
//!
//! Only a signed-in viewer can add one, so an endpoint is not trusted to be
//! benign so much as checked for shape: an https URL on a named host. A host
//! allowlist would add little — that viewer already runs shell commands on this
//! machine — and would break on any browser whose push service it did not list.

use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::Mutex;

/// More than any one person's browsers; a cap so a script cannot grow the file
/// without bound. The oldest is dropped to make room.
pub(super) const MAX_SUBSCRIPTIONS: usize = 32;
const MAX_ENDPOINT_LEN: usize = 2048;

/// One browser's subscription, as `PushSubscription.toJSON()` gives it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Subscription {
    pub endpoint: String,
    pub keys: SubscriptionKeys,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SubscriptionKeys {
    /// The browser's P-256 key, base64url.
    pub p256dh: String,
    /// The browser's 16-byte auth secret, base64url.
    pub auth: String,
}

impl Subscription {
    /// Why this subscription cannot be used, if it cannot.
    pub(super) fn problem(&self) -> Option<&'static str> {
        if self.endpoint.len() > MAX_ENDPOINT_LEN {
            return Some("push endpoint is too long");
        }
        if !endpoint_is_acceptable(&self.endpoint) {
            return Some("push endpoint must be an https URL on a public host name");
        }
        let key = URL_SAFE_NO_PAD.decode(self.keys.p256dh.trim_end_matches('='));
        if key.map_or(true, |k| k.len() != super::crypto::PUBLIC_KEY_LEN) {
            return Some("push key is not a P-256 public key");
        }
        let auth = URL_SAFE_NO_PAD.decode(self.keys.auth.trim_end_matches('='));
        if auth.map_or(true, |a| a.len() != 16) {
            return Some("push auth secret is not 16 bytes");
        }
        None
    }
}

/// `https://` with a host name that is not an address or a local name.
fn endpoint_is_acceptable(endpoint: &str) -> bool {
    let Some(rest) = endpoint.strip_prefix("https://") else {
        return false;
    };
    let authority = rest.split(['/', '?', '#']).next().unwrap_or("");
    if authority.contains('@') {
        return false;
    }
    let host = authority
        .rsplit_once(':')
        .map_or(authority, |(host, _port)| host)
        .to_ascii_lowercase();
    let local = host == "localhost" || host.ends_with(".localhost") || host.ends_with(".local");
    let literal = host.starts_with('[') || host.parse::<std::net::Ipv4Addr>().is_ok();
    host.contains('.') && !local && !literal
}

/// The subscription list, mirrored to `path` after every change.
pub(super) struct Store {
    path: Option<PathBuf>,
    subscriptions: Mutex<Vec<Subscription>>,
}

impl Store {
    /// Read `path`. A missing or unreadable file starts empty; an unreadable
    /// one is logged and replaced on the next change.
    pub(super) fn open(path: Option<PathBuf>) -> Self {
        let subscriptions = path
            .as_ref()
            .and_then(|path| match std::fs::read(path) {
                Ok(bytes) => serde_json::from_slice::<Vec<Subscription>>(&bytes)
                    .inspect_err(|err| tracing::warn!(%err, ?path, "push subscriptions unreadable"))
                    .ok(),
                Err(_) => None,
            })
            .unwrap_or_default();
        Self {
            path,
            subscriptions: Mutex::new(subscriptions),
        }
    }

    pub(super) fn all(&self) -> Vec<Subscription> {
        self.subscriptions
            .lock()
            .expect("push store poisoned")
            .clone()
    }

    pub(super) fn contains(&self, endpoint: &str) -> bool {
        self.subscriptions
            .lock()
            .expect("push store poisoned")
            .iter()
            .any(|s| s.endpoint == endpoint)
    }

    /// Add or refresh `subscription` (an endpoint's keys can be rotated).
    pub(super) fn add(&self, subscription: Subscription) {
        let mut all = self.subscriptions.lock().expect("push store poisoned");
        all.retain(|s| s.endpoint != subscription.endpoint);
        all.push(subscription);
        let excess = all.len().saturating_sub(MAX_SUBSCRIPTIONS);
        all.drain(..excess);
        self.persist(&all);
    }

    /// Drop `endpoint`. `false` when it was not there.
    pub(super) fn remove(&self, endpoint: &str) -> bool {
        let mut all = self.subscriptions.lock().expect("push store poisoned");
        let before = all.len();
        all.retain(|s| s.endpoint != endpoint);
        let removed = all.len() != before;
        if removed {
            self.persist(&all);
        }
        removed
    }

    fn persist(&self, all: &[Subscription]) {
        let Some(path) = &self.path else { return };
        let Ok(json) = serde_json::to_vec(all) else {
            return;
        };
        if let Err(err) = crate::platform::fs::write_atomic(path, &json) {
            tracing::warn!(%err, ?path, "could not persist push subscriptions");
        }
    }
}

#[cfg(test)]
#[path = "store_tests.rs"]
pub(super) mod tests;
