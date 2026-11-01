//! Persistent session token store. Tokens are opaque 256-bit random strings
//! backed by a file so they survive daemon restarts, each carrying the expiry
//! it was issued with. The lifetime is handed to the store at construction:
//! how long a login lasts is the operator's call, and `None` means tokens
//! never expire on their own.
//!
//! Logout revokes a token server-side — clearing the cookie alone leaves a
//! leaked token usable until expiry. Expired tokens are swept on every write
//! and on load; nothing is scheduled, since the file only changes when someone
//! logs in, logs out, or presents an expired token.
//!
//! The store is bounded: past [`MAX_SESSIONS`] the oldest login is forgotten,
//! in memory and in the file, so repeated logins cannot grow the file without
//! limit even when tokens never expire.
//!
//! The file is written owner-only (0o600 on Unix; see `platform::fs`). On
//! Windows that call is a no-op (documented there), so operators should place
//! the state directory in a restricted location. With `store_path` `None` the
//! store is in-memory only, for tests and transient runs.

use crate::platform;
use anyhow::{Context, Result, anyhow};
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

/// The most logins one server keeps. Each login adds a token and rewrites the
/// file, so a script could otherwise grow it without bound; the oldest is
/// forgotten to make room, as the push subscription store does.
const MAX_SESSIONS: usize = 64;

/// The longest lifetime a cookie may usefully ask for: RFC 6265bis caps
/// `Max-Age` at 400 days and Chrome has enforced it since 104. Asking for more
/// is silently reduced there and honoured elsewhere, so the ceiling is applied
/// here to keep what is sent equal to what any browser will do with it.
const MAX_COOKIE_AGE: Duration = Duration::from_secs(400 * 86400);

/// On-disk marker for a token with no expiry. Not a number, so a build that
/// predates this simply drops the line as it does any other it cannot parse.
const NEVER: &str = "never";

/// When a token stops being valid. `None` is "not on its own" — only logout or
/// a tightened lifetime ends it.
type Expiry = Option<SystemTime>;

/// One login, kept in the order it was issued.
struct Token {
    value: String,
    expiry: Expiry,
}

pub struct SessionStore {
    tokens: Mutex<Vec<Token>>,
    store_path: Option<PathBuf>,
    ttl: Option<Duration>,
}

impl SessionStore {
    /// In-memory only (no disk persistence). Used by tests and transient
    /// runs where restart-survival is not needed.
    pub fn new(ttl: Option<Duration>) -> Self {
        Self {
            tokens: Mutex::new(Vec::new()),
            store_path: None,
            ttl,
        }
    }

    /// Load tokens from `path`, discarding any that are already expired. If
    /// the file is missing or unreadable, start empty — a corrupt session
    /// file should not prevent the server from starting.
    pub fn load(path: PathBuf, ttl: Option<Duration>) -> Self {
        let mut tokens = Vec::new();
        let mut changed = false;
        if let Ok(data) = std::fs::read_to_string(&path) {
            for line in data.lines() {
                if let Some((token, expiry_str)) = line.split_once('\t')
                    && let Some(expiry) = parse_expiry(expiry_str.trim())
                {
                    tokens.push(Token {
                        value: token.to_string(),
                        expiry,
                    });
                }
            }
            let now = SystemTime::now();
            let before = tokens.len();
            changed = clamp(&mut tokens, ttl, now);
            trim(&mut tokens, MAX_SESSIONS, now);
            changed |= tokens.len() != before;
        }
        let store = Self {
            tokens: Mutex::new(tokens),
            store_path: Some(path),
            ttl,
        };
        // Write the removals and shortened deadlines down now instead of
        // leaving them to the next login: until then the file still names the
        // old ones, so a second restart would measure a fresh lifetime from
        // there and a tightened policy would never take hold on a
        // fast-restarting session. A token the cap dropped must not return.
        if changed {
            store.persist();
        }
        store
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
            tokens.push(Token {
                value: token.clone(),
                expiry,
            });
            trim(&mut tokens, MAX_SESSIONS, SystemTime::now());
        }
        self.persist();
        Ok(token)
    }

    /// Invalidate a token server-side. Clearing the cookie alone leaves a
    /// leaked token usable until expiry.
    pub fn revoke(&self, token: &str) {
        {
            let mut tokens = self.tokens.lock().expect("session store mutex poisoned");
            tokens.retain(|t| t.value != token);
        }
        self.persist();
    }

    pub fn is_valid(&self, token: &str) -> bool {
        let tokens = self.tokens.lock().expect("session store mutex poisoned");
        let now = SystemTime::now();
        let Some(found) = tokens.iter().find(|t| t.value == token) else {
            return false;
        };
        if found.expiry.is_none_or(|at| at > now) {
            return true;
        }
        // Expired. Dropping it is left to the write below, which forgets this
        // one and every other token that has run out with it.
        drop(tokens);
        self.persist();
        false
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
            // token and is about to write them down; otherwise a token nobody
            // asks about again would sit in memory and on disk until restart.
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
/// Only ever lowers: a token issued under a longer lifetime should not
/// outlive a policy the operator has since tightened, and a token already
/// closer to running out keeps its earlier deadline. Reports whether anything
/// moved, which is what tells the caller the file no longer matches the tokens.
fn clamp(tokens: &mut [Token], ttl: Option<Duration>, now: SystemTime) -> bool {
    let Some(ceiling) = ttl.and_then(|ttl| now.checked_add(ttl)) else {
        return false;
    };
    let mut shortened = false;
    for token in tokens.iter_mut() {
        if token.expiry.is_none_or(|at| at > ceiling) {
            token.expiry = Some(ceiling);
            shortened = true;
        }
    }
    shortened
}

/// Forget expired tokens, then the oldest logins beyond `cap`. Tokens are kept
/// in issue order, so the front of the list is what makes room.
fn trim(tokens: &mut Vec<Token>, cap: usize, now: SystemTime) {
    sweep(tokens, now);
    let excess = tokens.len().saturating_sub(cap);
    tokens.drain(..excess);
}

/// Forget every token whose expiry has passed. One rule, one place: loading and
/// writing both apply it, so a token's being on disk means the same thing as its
/// being in memory.
fn sweep(tokens: &mut Vec<Token>, now: SystemTime) {
    tokens.retain(|token| token.expiry.is_none_or(|at| at > now));
}

fn serialize(tokens: &[Token]) -> String {
    let mut out = String::new();
    for token in tokens {
        out.push_str(&token.value);
        out.push('\t');
        match token.expiry {
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

#[cfg(test)]
#[path = "sessions_cap_tests.rs"]
mod cap_tests;
