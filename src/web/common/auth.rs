//! Authentication for nightcrow's web servers: Argon2 password verification
//! (matching code-server's scheme) and login rate limiting. Session tokens
//! live in `sessions.rs`. The cookie name belongs to each server, not here.

use anyhow::{Result, anyhow};
use argon2::Argon2;
use argon2::password_hash::{PasswordHasher, PasswordVerifier, phc::PasswordHash};
use std::collections::VecDeque;
use std::sync::Mutex;
use std::time::{Duration, Instant};

/// A verifier for the single configured web password. Plaintext passwords are
/// hashed once at construction so every check runs through the same
/// constant-time Argon2 verify as a pre-hashed credential.
pub struct Auth {
    phc: String,
}

impl Auth {
    /// Hash a plaintext password into a PHC string using a fresh random salt.
    pub fn from_plaintext(password: &str) -> Result<Self> {
        let mut salt_bytes = [0u8; 16];
        getrandom::fill(&mut salt_bytes)
            .map_err(|e| anyhow!("OS RNG unavailable for password salt: {e}"))?;
        let phc = Argon2::default()
            .hash_password_with_salt(password.as_bytes(), &salt_bytes)
            .map_err(|e| anyhow!("hashing web password: {e}"))?
            .to_string();
        Ok(Self { phc })
    }

    /// Adopt a pre-computed Argon2 PHC hash, validating that it parses.
    pub fn from_hashed(phc: &str) -> Result<Self> {
        PasswordHash::new(phc)
            .map_err(|e| anyhow!("web.hashed_password is not a valid Argon2 PHC string: {e}"))?;
        Ok(Self {
            phc: phc.to_string(),
        })
    }

    /// Constant-time check of a submitted password against the stored hash.
    pub fn verify(&self, candidate: &str) -> bool {
        match PasswordHash::new(&self.phc) {
            Ok(parsed) => Argon2::default()
                .verify_password(candidate.as_bytes(), &parsed)
                .is_ok(),
            Err(_) => false,
        }
    }
}

/// Login rate limiter mirroring code-server: at most 2 attempts per minute and
/// 14 per hour (2/min baseline plus 12 additional/hour). Shared across all
/// clients since there is a single password.
pub struct RateLimiter {
    attempts: Mutex<VecDeque<Instant>>,
}

const MAX_PER_MINUTE: usize = 2;
const MAX_PER_HOUR: usize = 14;

impl RateLimiter {
    pub fn new() -> Self {
        Self {
            attempts: Mutex::new(VecDeque::with_capacity(MAX_PER_HOUR)),
        }
    }

    /// Record an attempt at time `now` and report whether it is allowed. A
    /// rejected attempt is still recorded so a flood cannot reset the window
    /// by simply retrying.
    pub fn check_and_record(&self, now: Instant) -> bool {
        let mut attempts = self.attempts.lock().expect("rate limiter mutex poisoned");
        attempts.retain(|t| now.duration_since(*t) < Duration::from_secs(3600));
        let last_minute = attempts
            .iter()
            .filter(|t| now.duration_since(**t) < Duration::from_secs(60))
            .count();
        let allowed = last_minute < MAX_PER_MINUTE && attempts.len() < MAX_PER_HOUR;
        attempts.push_back(now);
        if attempts.len() > MAX_PER_HOUR {
            attempts.pop_front();
        }
        allowed
    }
}

impl Default for RateLimiter {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
#[path = "auth_tests.rs"]
mod tests;
