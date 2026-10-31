//! The one thread that talks to push services.

use super::{Job, Payload, Shared, Subscription};
use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use std::collections::HashMap;
use std::sync::mpsc::Receiver;
use std::time::{Duration, Instant, SystemTime};

/// A pane that asks again inside this window is not sent again: an agent's
/// hook can fire on every turn, and the device keeps only the newest per tag
/// anyway.
pub(super) const COALESCE_WINDOW: Duration = Duration::from_secs(10);
/// How long a push service holds a message for a device that is offline.
const TTL_SECONDS: &str = "86400";
const REQUEST_TIMEOUT: Duration = Duration::from_secs(10);

/// What a push service said about one delivery.
#[derive(Debug, PartialEq, Eq)]
pub(super) enum Outcome {
    Delivered,
    /// 404 or 410: the browser unsubscribed or the subscription expired.
    Gone,
    Failed(String),
}

pub(super) fn run(shared: &Shared, jobs: Receiver<Job>) {
    let agent: ureq::Agent = ureq::Agent::config_builder()
        .https_only(true)
        .http_status_as_error(false)
        .timeout_global(Some(REQUEST_TIMEOUT))
        .build()
        .into();
    let mut last_sent: HashMap<String, Instant> = HashMap::new();
    for job in jobs {
        let (targets, payload) = match job {
            Job::Broadcast(payload) => {
                let now = Instant::now();
                if !due(&mut last_sent, &payload.tag, now) {
                    continue;
                }
                (shared.store.all(), payload)
            }
            Job::Test { endpoint, payload } => {
                let targets = shared
                    .store
                    .all()
                    .into_iter()
                    .filter(|s| s.endpoint == endpoint);
                (targets.collect(), payload)
            }
        };
        for subscription in targets {
            match deliver(&agent, shared, &subscription, &payload) {
                Outcome::Delivered => {
                    tracing::debug!(endpoint = %host(&subscription.endpoint), "push: delivered");
                }
                Outcome::Gone => {
                    tracing::info!(endpoint = %host(&subscription.endpoint), "push: subscription gone; removed");
                    shared.store.remove(&subscription.endpoint);
                }
                Outcome::Failed(why) => {
                    tracing::warn!(endpoint = %host(&subscription.endpoint), %why, "push: delivery failed");
                }
            }
        }
    }
}

/// Whether `tag` may be sent at `now`, recording it when it may.
pub(super) fn due(last_sent: &mut HashMap<String, Instant>, tag: &str, now: Instant) -> bool {
    last_sent.retain(|_, at| now.duration_since(*at) < COALESCE_WINDOW);
    if last_sent.contains_key(tag) {
        return false;
    }
    last_sent.insert(tag.to_string(), now);
    true
}

fn deliver(
    agent: &ureq::Agent,
    shared: &Shared,
    subscription: &Subscription,
    payload: &Payload,
) -> Outcome {
    let body = match encrypted(subscription, payload) {
        Ok(body) => body,
        Err(err) => return Outcome::Failed(format!("{err:#}")),
    };
    let authorization = match shared
        .vapid
        .authorization(&subscription.endpoint, SystemTime::now())
    {
        Ok(value) => value,
        Err(err) => return Outcome::Failed(format!("{err:#}")),
    };
    let response = agent
        .post(&subscription.endpoint)
        .header("TTL", TTL_SECONDS)
        .header("Urgency", "high")
        .header("Topic", topic(&payload.tag))
        .header("Content-Encoding", "aes128gcm")
        .header("Content-Type", "application/octet-stream")
        .header("Authorization", &authorization)
        .send(&body[..]);
    match response {
        Ok(response) => classify(response.status().as_u16()),
        Err(err) => Outcome::Failed(err.to_string()),
    }
}

pub(super) fn classify(status: u16) -> Outcome {
    match status {
        200..=299 => Outcome::Delivered,
        404 | 410 => Outcome::Gone,
        other => Outcome::Failed(format!("HTTP {other}")),
    }
}

fn encrypted(subscription: &Subscription, payload: &Payload) -> anyhow::Result<Vec<u8>> {
    let public_key = URL_SAFE_NO_PAD.decode(subscription.keys.p256dh.trim_end_matches('='))?;
    let auth_secret = URL_SAFE_NO_PAD.decode(subscription.keys.auth.trim_end_matches('='))?;
    let plaintext = serde_json::to_vec(payload)?;
    super::crypto::encrypt(
        &super::crypto::Recipient {
            public_key: &public_key,
            auth_secret: &auth_secret,
        },
        &plaintext,
    )
}

/// RFC 8030 `Topic`: a pending message with the same topic is replaced at the
/// push service, so a device that was offline gets one per pane, not a
/// backlog. At most 32 base64url characters.
pub(super) fn topic(tag: &str) -> String {
    use sha2::{Digest, Sha256};
    let digest = Sha256::digest(tag.as_bytes());
    URL_SAFE_NO_PAD.encode(&digest[..24])
}

/// The endpoint's host, for logs: the path is a per-browser capability.
fn host(endpoint: &str) -> &str {
    super::vapid::origin(endpoint).unwrap_or("?")
}

#[cfg(test)]
#[path = "sender_tests.rs"]
mod tests;
