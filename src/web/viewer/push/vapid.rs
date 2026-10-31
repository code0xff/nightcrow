//! The daemon's Web Push identity (VAPID, RFC 8292).
//!
//! One P-256 key per installation, made on first use and kept owner-only. A
//! browser's subscription is bound to its public half, so replacing the key
//! silently ends every subscription; it is therefore read back, never remade,
//! while the file is readable.

use super::crypto::{random_secret_key, uncompressed};
use anyhow::{Context, Result, anyhow};
use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use p256::SecretKey;
use p256::ecdsa::signature::Signer;
use p256::ecdsa::{Signature, SigningKey};
use std::path::Path;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

/// How long one signed token is good for. RFC 8292 caps it at 24 hours.
const TOKEN_LIFETIME: Duration = Duration::from_secs(12 * 60 * 60);
/// The contact a push service may use about this sender. Apple requires one.
const CONTACT: &str = "https://github.com/code0xff/nightcrow";

pub(super) struct Vapid {
    key: SigningKey,
    /// The public key, base64url, as browsers and the `k=` parameter take it.
    public: String,
}

impl Vapid {
    /// Read the key at `path`, or make and store one when there is none.
    /// A file that exists but cannot be read is an error, not a reason to
    /// replace it.
    pub(super) fn load_or_create(path: &Path) -> Result<Self> {
        let secret = match std::fs::read_to_string(path) {
            Ok(text) => {
                let bytes = URL_SAFE_NO_PAD
                    .decode(text.trim())
                    .context("push key file is not base64url")?;
                SecretKey::from_slice(&bytes)
                    .map_err(|_| anyhow!("push key file is not a P-256 key"))?
            }
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => {
                let secret = random_secret_key()?;
                let text = URL_SAFE_NO_PAD.encode(secret.to_bytes());
                crate::platform::fs::write_atomic(path, text.as_bytes())
                    .with_context(|| format!("writing push key {}", path.display()))?;
                secret
            }
            Err(err) => {
                return Err(err).with_context(|| format!("reading push key {}", path.display()));
            }
        };
        Ok(Self::from_secret(&secret))
    }

    pub(super) fn from_secret(secret: &SecretKey) -> Self {
        Self {
            public: URL_SAFE_NO_PAD.encode(uncompressed(&secret.public_key())),
            key: SigningKey::from(secret),
        }
    }

    pub(super) fn public_key(&self) -> &str {
        &self.public
    }

    /// The `Authorization` header value for a request to `endpoint`'s origin.
    pub(super) fn authorization(&self, endpoint: &str, now: SystemTime) -> Result<String> {
        let audience = origin(endpoint).ok_or_else(|| anyhow!("push endpoint has no origin"))?;
        let exp = (now + TOKEN_LIFETIME)
            .duration_since(UNIX_EPOCH)
            .map_err(|_| anyhow!("clock before the epoch"))?
            .as_secs();
        let header = URL_SAFE_NO_PAD.encode(br#"{"typ":"JWT","alg":"ES256"}"#);
        let claims = serde_json::json!({ "aud": audience, "exp": exp, "sub": CONTACT });
        let claims = URL_SAFE_NO_PAD.encode(claims.to_string());
        let signed = format!("{header}.{claims}");
        let signature: Signature = self.key.sign(signed.as_bytes());
        let signature = URL_SAFE_NO_PAD.encode(signature.to_bytes());
        Ok(format!("vapid t={signed}.{signature}, k={}", self.public))
    }
}

/// `scheme://host[:port]` of an absolute URL.
pub(super) fn origin(url: &str) -> Option<&str> {
    let after_scheme = url.find("://")? + 3;
    let end = url[after_scheme..]
        .find('/')
        .map_or(url.len(), |i| after_scheme + i);
    (end > after_scheme).then(|| &url[..end])
}

#[cfg(test)]
#[path = "vapid_tests.rs"]
mod tests;
