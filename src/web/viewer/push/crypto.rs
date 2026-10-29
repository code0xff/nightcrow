//! Message encryption for Web Push (RFC 8291, `aes128gcm` from RFC 8188).
//!
//! The push service relays the body but cannot read it: it is encrypted to the
//! browser's own key, which the browser handed over with its subscription.

use aes_gcm::aead::{Aead, KeyInit};
use aes_gcm::{Aes128Gcm, Nonce};
use anyhow::{Context, Result, anyhow, bail};
use hkdf::Hkdf;
use p256::{PublicKey, SecretKey};
use sha2::Sha256;

/// Record size written to the header. One record holds every notice: the
/// payload is far below it.
const RECORD_SIZE: u32 = 4096;
/// An uncompressed P-256 point: `0x04 || x || y`.
pub(super) const PUBLIC_KEY_LEN: usize = 65;
const AUTH_SECRET_LEN: usize = 16;
const SALT_LEN: usize = 16;

/// What a browser's subscription says to encrypt to.
pub(super) struct Recipient<'a> {
    /// The browser's `p256dh` key, uncompressed.
    pub(super) public_key: &'a [u8],
    /// The browser's `auth` secret.
    pub(super) auth_secret: &'a [u8],
}

/// Encrypt `plaintext` for `recipient` with a fresh key pair and salt.
pub(super) fn encrypt(recipient: &Recipient<'_>, plaintext: &[u8]) -> Result<Vec<u8>> {
    let sender = random_secret_key()?;
    let mut salt = [0u8; SALT_LEN];
    getrandom::fill(&mut salt).map_err(|e| anyhow!("OS RNG unavailable for push salt: {e}"))?;
    encrypt_with(recipient, plaintext, &sender, &salt)
}

/// [`encrypt`] with the sender key and salt chosen by the caller, so the RFC's
/// worked example can be reproduced byte for byte.
pub(super) fn encrypt_with(
    recipient: &Recipient<'_>,
    plaintext: &[u8],
    sender: &SecretKey,
    salt: &[u8; SALT_LEN],
) -> Result<Vec<u8>> {
    if recipient.public_key.len() != PUBLIC_KEY_LEN {
        bail!("subscription key is not an uncompressed P-256 point");
    }
    if recipient.auth_secret.len() != AUTH_SECRET_LEN {
        bail!("subscription auth secret is not {AUTH_SECRET_LEN} bytes");
    }
    let receiver = PublicKey::from_sec1_bytes(recipient.public_key)
        .context("subscription key is not on the P-256 curve")?;
    let sender_public = uncompressed(&sender.public_key());
    let shared = p256::ecdh::diffie_hellman(sender.to_nonzero_scalar(), receiver.as_affine());

    // RFC 8291 §3.4: mix the browser's auth secret into the ECDH output.
    let mut key_info = Vec::with_capacity(14 + 2 * PUBLIC_KEY_LEN);
    key_info.extend_from_slice(b"WebPush: info\0");
    key_info.extend_from_slice(recipient.public_key);
    key_info.extend_from_slice(&sender_public);
    let mut ikm = [0u8; 32];
    Hkdf::<Sha256>::new(Some(recipient.auth_secret), shared.raw_secret_bytes())
        .expand(&key_info, &mut ikm)
        .map_err(|_| anyhow!("HKDF output length rejected"))?;

    // RFC 8188 §2.2: content key and nonce from the salt.
    let prk = Hkdf::<Sha256>::new(Some(salt), &ikm);
    let mut cek = [0u8; 16];
    let mut nonce = [0u8; 12];
    prk.expand(b"Content-Encoding: aes128gcm\0", &mut cek)
        .and_then(|()| prk.expand(b"Content-Encoding: nonce\0", &mut nonce))
        .map_err(|_| anyhow!("HKDF output length rejected"))?;

    // A single, final record: the content, then the `0x02` last-record delimiter.
    let mut record = Vec::with_capacity(plaintext.len() + 1);
    record.extend_from_slice(plaintext);
    record.push(0x02);
    let cipher = Aes128Gcm::new_from_slice(&cek).map_err(|_| anyhow!("bad AES key length"))?;
    let sealed = cipher
        .encrypt(&Nonce::from(nonce), record.as_slice())
        .map_err(|_| anyhow!("AES-GCM encryption failed"))?;

    let mut body = Vec::with_capacity(SALT_LEN + 5 + PUBLIC_KEY_LEN + sealed.len());
    body.extend_from_slice(salt);
    body.extend_from_slice(&RECORD_SIZE.to_be_bytes());
    body.push(PUBLIC_KEY_LEN as u8);
    body.extend_from_slice(&sender_public);
    body.extend_from_slice(&sealed);
    Ok(body)
}

/// A P-256 public key as the 65-byte uncompressed point Web Push exchanges.
pub(super) fn uncompressed(key: &PublicKey) -> Vec<u8> {
    key.to_sec1_bytes().to_vec()
}

/// A secret key from OS entropy. A 32-byte draw is outside the curve order
/// with negligible probability; drawing again covers it.
pub(super) fn random_secret_key() -> Result<SecretKey> {
    for _ in 0..8 {
        let mut bytes = [0u8; 32];
        getrandom::fill(&mut bytes).map_err(|e| anyhow!("OS RNG unavailable for push key: {e}"))?;
        if let Ok(key) = SecretKey::from_slice(&bytes) {
            return Ok(key);
        }
    }
    bail!("could not draw a valid P-256 key")
}

#[cfg(test)]
#[path = "crypto_tests.rs"]
mod tests;
