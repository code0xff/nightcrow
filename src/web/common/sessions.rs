//! Persistent session token store. Tokens are opaque 256-bit random strings
//! backed by a file so they survive daemon restarts. Each token carries an
//! expiry (TTL) so a restarted server does not accept stale tokens forever.
//!
//! Logout revokes a token server-side — clearing the cookie alone is not
//! enough because a leaked token would remain usable until expiry. Revocation
//! removes the token from both memory and the on-disk store.
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

/// Session lifetime: 24 hours, matching the cookie Max-Age.
pub const SESSION_TTL: Duration = Duration::from_secs(86400);

pub struct SessionStore {
    tokens: Mutex<HashMap<String, SystemTime>>,
    store_path: Option<PathBuf>,
}

impl SessionStore {
    /// In-memory only (no disk persistence). Used by tests and transient
    /// runs where restart-survival is not needed.
    pub fn new() -> Self {
        Self {
            tokens: Mutex::new(HashMap::new()),
            store_path: None,
        }
    }

    /// Load tokens from `path`, discarding any that are already expired. If
    /// the file is missing or unreadable, start empty — a corrupt session
    /// file should not prevent the server from starting.
    pub fn load(path: PathBuf) -> Self {
        let mut tokens = HashMap::new();
        if let Ok(data) = std::fs::read_to_string(&path) {
            let now = SystemTime::now();
            for line in data.lines() {
                if let Some((token, expiry_str)) = line.split_once('\t')
                    && let Ok(secs) = expiry_str.trim().parse::<u64>()
                {
                    let expiry = UNIX_EPOCH + Duration::from_secs(secs);
                    if expiry > now {
                        tokens.insert(token.to_string(), expiry);
                    }
                }
            }
        }
        Self {
            tokens: Mutex::new(tokens),
            store_path: Some(path),
        }
    }

    /// Mint a new session token and remember it.
    pub fn issue(&self) -> Result<String> {
        let mut bytes = [0u8; 32];
        getrandom::fill(&mut bytes)
            .map_err(|e| anyhow!("OS RNG unavailable for session token: {e}"))?;
        let token = hex(&bytes);
        let expiry = SystemTime::now() + SESSION_TTL;
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
        let mut tokens = self.tokens.lock().expect("session store mutex poisoned");
        let now = SystemTime::now();
        match tokens.get(token) {
            Some(expiry) if *expiry > now => true,
            Some(_) => {
                // Expired — remove lazily.
                tokens.remove(token);
                drop(tokens);
                self.persist();
                false
            }
            None => false,
        }
    }

    /// Write the current token set to disk. Failures are logged but not
    /// propagated: a session that fails to persist is still valid in memory
    /// — the user just loses restart-survival, not access.
    fn persist(&self) {
        let Some(path) = &self.store_path else { return };
        let data = self.serialize();
        if let Err(err) = platform::fs::write_atomic(path, data.as_bytes()) {
            tracing::warn!(%err, ?path, "could not persist session store");
        }
    }

    fn serialize(&self) -> String {
        let tokens = self.tokens.lock().expect("session store mutex poisoned");
        let mut out = String::new();
        for (token, expiry) in tokens.iter() {
            let secs = expiry
                .duration_since(UNIX_EPOCH)
                .map(|d| d.as_secs())
                .unwrap_or(0);
            out.push_str(token);
            out.push('\t');
            out.push_str(&secs.to_string());
            out.push('\n');
        }
        out
    }
}

impl Default for SessionStore {
    fn default() -> Self {
        Self::new()
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
