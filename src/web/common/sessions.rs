//! Persistent session token store. Tokens are opaque 256-bit random strings
//! backed by a file so they survive daemon restarts. Each token carries the
//! expiry it was issued with, so a restarted server does not accept stale
//! tokens forever.
//!
//! The lifetime is the store's, handed to it at construction rather than fixed
//! here: how long a login should last is the operator's call, and a store told
//! `None` issues tokens that never expire on their own.
//!
//! Logout revokes a token server-side — clearing the cookie alone is not
//! enough because a leaked token would remain usable until expiry. Revocation
//! removes the token from both memory and the on-disk store.
//!
//! Expired tokens are forgotten opportunistically: every write sweeps the whole
//! set, and loading discards what has already run out. Nothing is scheduled —
//! the file only changes when someone logs in, logs out, or presents a token
//! that has expired, and those are the moments worth paying for the sweep.
//!
//! The file is written with owner-only permissions (0o600 on Unix); see
//! `platform::fs`. On Windows the permission call is a no-op (documented
//! there), so operators should place the state directory in a restricted
//! location.
//!
//! When `store_path` is `None` the store is in-memory only, matching the old
//! behaviour for tests and transient runs.

use crate::platform;
use anyhow::{Context, Result, anyhow};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

/// Session lifetime used when the config names none.
pub const DEFAULT_SESSION_TTL: Duration = Duration::from_secs(86400);

/// Browsers refuse to keep a cookie beyond 400 days (RFC 6265bis, enforced by
/// Chrome 104+ and Safari), so that is the ceiling on `Max-Age` however long
/// the server is willing to honour the token behind it.
const MAX_COOKIE_AGE: Duration = Duration::from_secs(400 * 86400);

/// On-disk marker for a token with no expiry. Not a number, so a build that
/// predates this simply drops the line as it does any other it cannot parse.
const NEVER: &str = "never";

/// When a token stops being valid. `None` is "not on its own" — only logout or
/// a tightened lifetime ends it.
type Expiry = Option<SystemTime>;

pub struct SessionStore {
    tokens: Mutex<HashMap<String, Expiry>>,
    store_path: Option<PathBuf>,
    ttl: Option<Duration>,
}

impl SessionStore {
    /// In-memory only (no disk persistence). Used by tests and transient
    /// runs where restart-survival is not needed.
    pub fn new(ttl: Option<Duration>) -> Self {
        Self {
            tokens: Mutex::new(HashMap::new()),
            store_path: None,
            ttl,
        }
    }

    /// Load tokens from `path`, discarding any that are already expired. If
    /// the file is missing or unreadable, start empty — a corrupt session
    /// file should not prevent the server from starting.
    pub fn load(path: PathBuf, ttl: Option<Duration>) -> Self {
        let mut tokens = HashMap::new();
        if let Ok(data) = std::fs::read_to_string(&path) {
            for line in data.lines() {
                if let Some((token, expiry_str)) = line.split_once('\t')
                    && let Some(expiry) = parse_expiry(expiry_str.trim())
                {
                    tokens.insert(token.to_string(), expiry);
                }
            }
            let now = SystemTime::now();
            clamp(&mut tokens, ttl, now);
            sweep(&mut tokens, now);
        }
        Self {
            tokens: Mutex::new(tokens),
            store_path: Some(path),
            ttl,
        }
    }

    /// Mint a new session token and remember it.
    pub fn issue(&self) -> Result<String> {
        let mut bytes = [0u8; 32];
        getrandom::fill(&mut bytes)
            .map_err(|e| anyhow!("OS RNG unavailable for session token: {e}"))?;
        let token = hex(&bytes);
        // `checked_add`, because `SystemTime + Duration` panics on overflow. A
        // clock that cannot hold now+lifetime is a broken clock, not a request
        // for an immortal session, so this refuses rather than issues one.
        let expiry = match self.ttl {
            Some(ttl) => Some(SystemTime::now().checked_add(ttl).ok_or_else(|| {
                anyhow!("session lifetime of {ttl:?} does not fit this machine's clock")
            })?),
            None => None,
        };
        {
            let mut tokens = self.tokens.lock().expect("session store mutex poisoned");
            tokens.insert(token.clone(), expiry);
        }
        self.persist();
        Ok(token)
    }

    /// Invalidate a token server-side. Clearing the cookie alone leaves a
    /// leaked token usable until expiry, which makes logout a suggestion
    /// rather than a revocation.
    pub fn revoke(&self, token: &str) {
        {
            let mut tokens = self.tokens.lock().expect("session store mutex poisoned");
            tokens.remove(token);
        }
        self.persist();
    }

    pub fn is_valid(&self, token: &str) -> bool {
        let tokens = self.tokens.lock().expect("session store mutex poisoned");
        let now = SystemTime::now();
        match tokens.get(token) {
            Some(None) => true,
            Some(Some(expiry)) if *expiry > now => true,
            Some(_) => {
                // Expired. Dropping it is left to the write below, which forgets
                // this one and every other token that has run out with it.
                drop(tokens);
                self.persist();
                false
            }
            None => false,
        }
    }

    /// How long the login cookie may ask the browser to keep it. A session that
    /// never expires still gets a bounded cookie, because no browser will hold
    /// one past the cap — the difference is that returning within it keeps
    /// working indefinitely rather than only until the server forgets.
    pub fn cookie_max_age_secs(&self) -> u64 {
        self.ttl
            .unwrap_or(MAX_COOKIE_AGE)
            .min(MAX_COOKIE_AGE)
            .as_secs()
    }

    /// Write the current token set to disk. Failures are logged but not
    /// propagated: a session that fails to persist is still valid in memory
    /// — the user just loses restart-survival, not access.
    fn persist(&self) {
        let data = {
            let mut tokens = self.tokens.lock().expect("session store mutex poisoned");
            // Swept here because this is the one place that already holds every
            // token and is about to write them down. `is_valid` only reaches the
            // token it was asked about, and a session nobody asks about again —
            // the browser holding that cookie never came back — would otherwise
            // sit in memory and on disk until the daemon restarts, so the file
            // would claim sessions that cannot log anyone in.
            //
            // Ahead of the store having a file: a store without one (the
            // fallback when the state directory cannot be opened) holds the same
            // tokens in the same map, and is the one that cannot be fixed by a
            // restart reading a swept file.
            sweep(&mut tokens, SystemTime::now());
            serialize(&tokens)
        };
        let Some(path) = &self.store_path else { return };
        if let Err(err) = platform::fs::write_atomic(path, data.as_bytes()) {
            tracing::warn!(%err, ?path, "could not persist session store");
        }
    }
}

/// Read one file field back into an expiry, or `None` for a line to drop.
///
/// The numeric branch uses `checked_add` because `UNIX_EPOCH + Duration` panics
/// on overflow: a file naming an expiry no clock can hold would otherwise take
/// the server down on the way up, which is the one thing reading this file is
/// not allowed to do. A line like that is corrupt, so it is dropped like any
/// other unreadable one.
fn parse_expiry(field: &str) -> Option<Expiry> {
    if field == NEVER {
        return Some(None);
    }
    let secs = field.parse::<u64>().ok()?;
    UNIX_EPOCH.checked_add(Duration::from_secs(secs)).map(Some)
}

/// Bring loaded expiries down to what the configured lifetime allows.
///
/// Only ever lowers. A token issued under a longer lifetime — or none at all —
/// should not outlive a policy the operator has since tightened, and tightening
/// it is the one edit that has to reach sessions already handed out. A token
/// already closer to running out keeps its own earlier deadline, so a restart
/// never extends anything.
fn clamp(tokens: &mut HashMap<String, Expiry>, ttl: Option<Duration>, now: SystemTime) {
    let Some(ceiling) = ttl.and_then(|ttl| now.checked_add(ttl)) else {
        return;
    };
    for expiry in tokens.values_mut() {
        if expiry.is_none_or(|at| at > ceiling) {
            *expiry = Some(ceiling);
        }
    }
}

/// Forget every token whose expiry has passed. One rule, one place: loading and
/// writing both apply it, so a token's being on disk means the same thing as its
/// being in memory.
fn sweep(tokens: &mut HashMap<String, Expiry>, now: SystemTime) {
    tokens.retain(|_, expiry| expiry.is_none_or(|at| at > now));
}

fn serialize(tokens: &HashMap<String, Expiry>) -> String {
    let mut out = String::new();
    for (token, expiry) in tokens.iter() {
        out.push_str(token);
        out.push('\t');
        match expiry {
            Some(at) => {
                let secs = at
                    .duration_since(UNIX_EPOCH)
                    .map(|d| d.as_secs())
                    .unwrap_or(0);
                out.push_str(&secs.to_string());
            }
            None => out.push_str(NEVER),
        }
        out.push('\n');
    }
    out
}

impl Default for SessionStore {
    fn default() -> Self {
        Self::new(Some(DEFAULT_SESSION_TTL))
    }
}

fn hex(bytes: &[u8]) -> String {
    let mut s = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        s.push_str(&format!("{b:02x}"));
    }
    s
}

/// Resolve the session store path relative to the nightcrow state directory.
pub fn session_store_path() -> Result<PathBuf> {
    let anchor = platform::paths::state_dir_anchor();
    let dir = Path::new(&anchor).join(".nightcrow");
    std::fs::create_dir_all(&dir).context("creating nightcrow state directory for sessions")?;
    Ok(dir.join("sessions"))
}

#[cfg(test)]
#[path = "sessions_tests.rs"]
mod tests;
